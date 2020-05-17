use crate::ecs::component::{Account, Connection};
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::WorldId;
use crate::ecs::system::{send_event, send_event_with_connection};
use crate::model;
use crate::model::repository::{account, loginticket};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::{bail, ensure, Context};
use async_std::sync::Sender;
use async_std::task;
use shipyard::*;
use sqlx::PgPool;
use std::time::Instant;
use tracing::{debug, error, info, info_span, trace};

const MAX_UNAUTHENTICATED_LIFETIME: u64 = 5;
const PING_INTERVAL: u64 = 60;
const PONG_DEADLINE: u64 = 75;

/// Connection manager handles the connection components.
pub fn connection_manager_system(
    incoming_events: View<EcsEvent>,
    mut accounts: ViewMut<Account>,
    mut connections: ViewMut<Connection>,
    mut entities: EntitiesViewMut,
    pool: UniqueView<PgPool>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    // Incoming events
    (&incoming_events).iter().for_each(|event| match &**event {
        Event::RequestRegisterConnection {
            response_channel, ..
        } => handle_connection_registration(
            response_channel.clone(),
            &mut connections,
            &mut entities,
        ),
        Event::RequestCheckVersion {
            connection_id,
            packet,
        } => {
            if let Err(e) = handle_request_check_version(*connection_id, &packet, &mut connections)
            {
                error!("Rejecting request check version event: {:?}", e);
                send_event(reject_check_version(*connection_id), &connections);
                drop_connection(*connection_id, &mut connections);
            }
        }
        Event::RequestLoginArbiter {
            connection_id,
            packet,
        } => {
            if let Err(e) = handle_request_login_arbiter(
                *connection_id,
                &packet,
                &mut accounts,
                &mut connections,
                &mut entities,
                &pool,
            ) {
                error!("Rejecting login arbiter event: {:?}", e);
                send_event(
                    reject_login_arbiter(*connection_id, -1, packet.region),
                    &connections,
                );
                drop_connection(*connection_id, &mut connections);
            }
        }
        Event::RequestPong { connection_id, .. } => handle_pong(*connection_id, &mut connections),
        _ => { /* Ignore all other packets */ }
    });

    // Check the status of the existing connections and drop inactive connections
    let now = Instant::now();

    // Ping/Pong test for authenticated connections
    let mut to_drop = Vec::new();
    (&mut connections)
        .iter()
        .with_id()
        .filter(|(_, connection)| connection.is_authenticated)
        .for_each(|(connection_id, mut connection)| {
            if handle_ping(&now, connection_id, &mut connection) {
                to_drop.push(connection_id);
            }
        });

    // Unauthenticated connections only live for 5 seconds
    (&mut connections)
        .iter()
        .with_id()
        .filter(|(_, connection)| !connection.is_authenticated)
        .for_each(|(connection_id, connection)| {
            let last_pong_duration = now.duration_since(connection.last_pong).as_secs();
            if last_pong_duration >= MAX_UNAUTHENTICATED_LIFETIME {
                to_drop.push(connection_id);
            }
        });

    for connection_id in to_drop {
        drop_connection(connection_id, &mut connections);
    }
}

fn handle_connection_registration(
    response_channel: Sender<EcsEvent>,
    connections: &mut ViewMut<Connection>,
    entities: &mut EntitiesViewMut,
) {
    debug!("Registration event incoming");

    // Create a new connection component to properly handle it's state
    let connection_id = entities.add_entity(
        &mut *connections,
        Connection {
            channel: response_channel,
            is_authenticated: false,
            is_version_checked: false,
            last_pong: Instant::now(),
            waiting_for_pong: false,
        },
    );

    debug!("Registered connection as {:?}", connection_id);
    send_event(accept_connection_registration(connection_id), &*connections);
}

fn handle_request_check_version(
    connection_id: EntityId,
    packet: &CCheckVersion,
    mut connections: &mut ViewMut<Connection>,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!("Check version event incoming");

    ensure!(
        packet.version.len() == 2,
        format!(
            "Expected version array to be of length 2 but is {}",
            packet.version.len()
        )
    );

    // TODO properly do the version verification? Define version 0 and 1 in the config file?
    debug!(
        "Version 1: {} version 2: {}",
        packet.version[0].value, packet.version[1].value
    );

    let mut connection = (&mut connections)
        .try_get(connection_id)
        .context("Could not find connection component for entity")?;
    connection.is_version_checked = true;

    Ok(())
}

fn handle_request_login_arbiter(
    connection_id: EntityId,
    packet: &CLoginArbiter,
    accounts: &mut ViewMut<Account>,
    mut connections: &mut ViewMut<Connection>,
    entities: &mut EntitiesViewMut,
    pool: &PgPool,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!(
        "Login arbiter event incoming for account: {}",
        packet.master_account_name
    );

    Ok(task::block_on(async {
        let mut connection = (&mut connections)
            .try_get(connection_id)
            .context("Could not find connection component for entity")?;

        trace!("Ticket value: {}", base64::encode(&packet.ticket));

        if packet.ticket.is_empty() {
            bail!("Ticket was empty");
        }

        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if !loginticket::is_ticket_valid(&mut conn, &packet.master_account_name, &packet.ticket)
            .await
            .context("Error while executing query for account")?
        {
            bail!("Ticket not valid");
        }

        info!(
            "Account {} provided a valid ticket",
            packet.master_account_name
        );

        let account = account::get_by_name(&mut conn, &packet.master_account_name)
            .await
            .context("Can't find the account for the given master account name")?;

        ensure!(
            accounts.iter().find(|id| id.id == account.id).is_none(),
            "Account is already logged in"
        );

        connection.is_authenticated = true;

        let account = Account {
            id: account.id,
            region: packet.region,
        };
        entities.add_component(accounts, account, connection_id);

        check_and_handle_post_initialization(connection_id, account, connection);

        Ok(())
    })?)
}

// Returns true if connection didn't return a ping in time.
fn handle_ping(now: &Instant, connection_id: EntityId, mut connection: &mut Connection) -> bool {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    let last_pong_duration = now.duration_since(connection.last_pong).as_secs();

    if last_pong_duration >= PONG_DEADLINE {
        debug!(
            "Didn't received pong in {} seconds. Dropping connection",
            PONG_DEADLINE
        );
        true
    } else if !connection.waiting_for_pong && last_pong_duration >= PING_INTERVAL {
        debug!("Sending ping");
        connection.waiting_for_pong = true;
        send_event_with_connection(assemble_ping(connection_id), connection);
        false
    } else {
        false
    }
}

fn handle_pong(connection_id: EntityId, mut connections: &mut ViewMut<Connection>) {
    debug!("Pong event incoming");

    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    if let Ok(mut connection) = (&mut connections).try_get(connection_id) {
        connection.last_pong = Instant::now();
        connection.waiting_for_pong = false;
    } else {
        error!("Could not find connection component for entity");
    }
}

fn drop_connection(connection_id: EntityId, connections: &mut ViewMut<Connection>) {
    send_event(assemble_drop_connection(connection_id), &*connections);
    connections.delete(connection_id);
}

fn check_and_handle_post_initialization(
    connection_id: EntityId,
    account: Account,
    connection: &Connection,
) {
    // Now that the client is vetted, we need to send him some specific packets in order for him to progress.
    debug!("Sending connection post initialization commands");

    // FIXME get from configuration (server name and PVP setting)!
    send_event_with_connection(accept_check_version(connection_id), &connection);
    send_event_with_connection(assemble_loading_screen_info(connection_id), &connection);
    send_event_with_connection(assemble_remain_play_time(connection_id), &connection);
    send_event_with_connection(
        accept_login_arbiter(connection_id, account.id, account.region),
        &connection,
    );
    send_event_with_connection(
        assemble_login_account_info(connection_id, "Almetica".to_string(), account.id),
        &connection,
    );
}

fn assemble_loading_screen_info(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseLoadingScreenControlInfo {
        connection_id,
        packet: SLoadingScreenControlInfo {
            custom_screen_enabled: false,
        },
    })
}

fn assemble_remain_play_time(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseRemainPlayTime {
        connection_id,
        packet: SRemainPlayTime {
            account_type: 6,
            minutes_left: 0,
        },
    })
}

fn assemble_login_account_info(
    connection_id: EntityId,
    server_name: String,
    account_id: i64,
) -> EcsEvent {
    Box::new(Event::ResponseLoginAccountInfo {
        connection_id,
        packet: SLoginAccountInfo {
            server_name,
            account_id,
            integrity_iv: 0x0, // We don't care for the integrity hash, since it's broken anyhow.
        },
    })
}

fn assemble_ping(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponsePing {
        connection_id,
        packet: SPing {},
    })
}

fn assemble_drop_connection(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseDropConnection { connection_id })
}

fn accept_connection_registration(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseRegisterConnection { connection_id })
}

fn accept_check_version(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseCheckVersion {
        connection_id,
        packet: SCheckVersion { ok: true },
    })
}

fn reject_check_version(connection_id: EntityId) -> EcsEvent {
    Box::new(Event::ResponseCheckVersion {
        connection_id,
        packet: SCheckVersion { ok: false },
    })
}

// TODO read PVP option out of configuration
fn accept_login_arbiter(
    connection_id: EntityId,
    account_id: i64,
    region: model::Region,
) -> EcsEvent {
    Box::new(Event::ResponseLoginArbiter {
        connection_id,
        account_id,
        packet: SLoginArbiter {
            success: true,
            login_queue: false,
            status: 65538,
            unk1: 0,
            region,
            pvp_disabled: false,
            unk2: 0,
            unk3: 0,
        },
    })
}

// TODO read PVP option out of configuration
fn reject_login_arbiter(
    connection_id: EntityId,
    account_id: i64,
    region: model::Region,
) -> EcsEvent {
    Box::new(Event::ResponseLoginArbiter {
        connection_id,
        account_id,
        packet: SLoginArbiter {
            success: false,
            login_queue: false,
            status: 0,
            unk1: 0,
            region,
            pvp_disabled: false,
            unk2: 0,
            unk3: 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::component;
    use crate::ecs::event::Event;
    use crate::ecs::resource::DeletionList;
    use crate::ecs::system::cleaner_system;
    use crate::model::entity;
    use crate::model::repository::account;
    use crate::model::repository::loginticket;
    use crate::model::tests::db_test;
    use crate::model::{PasswordHashAlgorithm, Region};
    use crate::protocol::packet::CCheckVersion;
    use crate::Result;
    use async_std::prelude::*;
    use async_std::sync::{channel, Receiver};
    use chrono::{TimeZone, Utc};
    use sqlx::pool::PoolConnection;
    use sqlx::{PgConnection, PgPool};
    use std::time::Duration;

    fn setup(pool: PgPool) -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));
        world.add_unique(pool);
        world
    }

    fn setup_with_connection(
        pool: PgPool,
        is_authenticated: bool,
    ) -> (World, EntityId, Receiver<EcsEvent>) {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(pool);

        let (tx_channel, rx_channel) = channel(1024);

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        channel: tx_channel,
                        is_authenticated,
                        is_version_checked: is_authenticated,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        (world, connection_id, rx_channel)
    }

    async fn create_login(conn: &mut PgConnection) -> Result<(entity::Account, Vec<u8>)> {
        let acc = account::create(
            conn,
            &entity::Account {
                id: -1,
                name: "testaccount".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?;
        let ticket = loginticket::upsert_ticket(conn, acc.id).await?;
        Ok((acc, ticket.ticket))
    }

    #[test]
    fn test_connection_registration() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let world = setup(pool);
                let (tx_channel, rx_channel) = channel(10);

                world.run(
                    |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                        for _i in 0..5 {
                            entities.add_entity(
                                &mut events,
                                Box::new(Event::RequestRegisterConnection {
                                    response_channel: tx_channel.clone(),
                                }),
                            );
                        }
                    },
                );

                world.run(connection_manager_system);

                let mut count = 0;
                loop {
                    if let Ok(event) = rx_channel.try_recv() {
                        match *event {
                            Event::ResponseRegisterConnection { .. } => count += 1,
                            _ => {}
                        }
                    } else {
                        break;
                    }
                }
                assert_eq!(count, 5);

                Ok(())
            })
        })
    }

    #[test]
    fn test_check_version_valid() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (world, connection_id, _rx_channel) = setup_with_connection(pool, true);

                world.run(
                    |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                        entities.add_entity(
                            &mut events,
                            Box::new(Event::RequestCheckVersion {
                                connection_id,
                                packet: CCheckVersion {
                                    version: vec![
                                        CCheckVersionEntry {
                                            index: 0,
                                            value: 366_222,
                                        },
                                        CCheckVersionEntry {
                                            index: 1,
                                            value: 365_535,
                                        },
                                    ],
                                },
                            }),
                        )
                    },
                );

                world.run(connection_manager_system);

                let valid_count = world
                    .borrow::<View<Connection>>()
                    .iter()
                    .filter(|connection| connection.is_version_checked)
                    .count();
                assert_eq!(valid_count, 1);

                Ok(())
            })
        })
    }

    #[test]
    fn test_check_version_invalid() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (world, connection_id, mut rx_channel) = setup_with_connection(pool, true);

                world.run(
                    |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                        entities.add_entity(
                            &mut events,
                            Box::new(Event::RequestCheckVersion {
                                connection_id,
                                packet: CCheckVersion {
                                    version: vec![CCheckVersionEntry {
                                        index: 0,
                                        value: 366_222,
                                    }],
                                },
                            }),
                        )
                    },
                );

                world.run(connection_manager_system);

                assert!(
                    rx_channel
                        .all(|event| match *event {
                            Event::ResponseCheckVersion { packet, .. } => !packet.ok,
                            Event::ResponseDropConnection { .. } => true,
                            _ => false,
                        })
                        .await,
                );

                // The connection should be dropped.
                let count = world.borrow::<View<Connection>>().iter().count();
                assert_eq!(count, 0);

                Ok(())
            })
        })
    }

    #[test]
    fn test_login_arbiter_valid() -> Result<()> {
        db_test(|db_string| {
            let (_conn, _rx_channel, world, connection_id, account, ticket) =
                task::block_on(async {
                    let pool = PgPool::new(db_string).await?;
                    let mut conn = pool.acquire().await?;
                    let (world, connection_id, rx_channel) = setup_with_connection(pool, true);
                    let (account, ticket) = create_login(&mut conn).await?;

                    Ok::<
                        (
                            PoolConnection<PgConnection>,
                            Receiver<EcsEvent>,
                            World,
                            EntityId,
                            entity::Account,
                            Vec<u8>,
                        ),
                        anyhow::Error,
                    >((conn, rx_channel, world, connection_id, account, ticket))
                })?;

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestLoginArbiter {
                            connection_id,
                            packet: CLoginArbiter {
                                master_account_name: account.name.clone(),
                                ticket,
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        }),
                    )
                },
            );

            world.run(connection_manager_system);

            let valid_count = world
                .borrow::<View<component::Account>>()
                .iter()
                .filter(|acc| acc.id == account.id && acc.region == Region::Europe)
                .count();
            assert_eq!(valid_count, 1);

            Ok(())
        })
    }

    #[test]
    fn test_login_arbiter_invalid() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_id, rx_channel) = setup_with_connection(pool, true);
            let (account, mut ticket) = task::block_on(async { create_login(&mut conn).await })?;

            // Make ticket invalid
            ticket.make_ascii_uppercase();

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestLoginArbiter {
                            connection_id,
                            packet: CLoginArbiter {
                                master_account_name: account.name,
                                ticket,
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        }),
                    )
                },
            );

            world.run(connection_manager_system);

            let mut count = 0;
            loop {
                if let Ok(event) = rx_channel.try_recv() {
                    match *event {
                        Event::ResponseLoginArbiter { packet, .. } => {
                            if !packet.success {
                                count += 1;
                            }
                        }
                        Event::ResponseDropConnection { .. } => {
                            count += 1;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
            assert_eq!(count, 2);

            // The connection should be dropped.
            let count = world.borrow::<View<Connection>>().iter().count();
            assert_eq!(count, 0);

            Ok(())
        })
    }

    #[test]
    fn test_login_arbiter_reject_double_login() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_id, rx_channel) = setup_with_connection(pool, true);
            let (account, ticket) = task::block_on(async { create_login(&mut conn).await })?;

            // Add an account component to the connection entity to signal that it's already logged in
            world.run(
                |entities: EntitiesViewMut, mut accounts: ViewMut<Account>| {
                    entities.add_component(
                        &mut accounts,
                        Account {
                            id: account.id,
                            region: Region::Europe,
                        },
                        connection_id,
                    )
                },
            );

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestLoginArbiter {
                            connection_id,
                            packet: CLoginArbiter {
                                master_account_name: account.name,
                                ticket,
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        }),
                    )
                },
            );

            world.run(connection_manager_system);

            let mut count = 0;
            loop {
                if let Ok(event) = rx_channel.try_recv() {
                    match *event {
                        Event::ResponseLoginArbiter { packet, .. } => {
                            if !packet.success {
                                count += 1;
                            }
                        }
                        Event::ResponseDropConnection { .. } => {
                            count += 1;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
            assert_eq!(count, 2);

            // The connection should be dropped.
            let count = world.borrow::<View<Connection>>().iter().count();
            assert_eq!(count, 0);

            Ok(())
        })
    }

    #[test]
    fn test_login_sequence() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let world = setup(pool);
            let (account, ticket) = task::block_on(async { create_login(&mut conn).await })?;
            let (tx_channel, rx_channel) = channel(10);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestRegisterConnection {
                            response_channel: tx_channel.clone(),
                        }),
                    )
                },
            );

            world.run(connection_manager_system);

            let con = match rx_channel.try_recv() {
                Ok(event) => match *event {
                    Event::ResponseRegisterConnection { connection_id } => connection_id,
                    _ => panic!("Received wrong event"),
                },
                _ => panic!("Couldn't find event"),
            };

            // Run the cleaner to clean up all events.
            world.run(cleaner_system);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestCheckVersion {
                            connection_id: con,
                            packet: CCheckVersion {
                                version: vec![
                                    CCheckVersionEntry {
                                        index: 0,
                                        value: 366_222,
                                    },
                                    CCheckVersionEntry {
                                        index: 1,
                                        value: 365_535,
                                    },
                                ],
                            },
                        }),
                    );
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestLoginArbiter {
                            connection_id: con,
                            packet: CLoginArbiter {
                                master_account_name: account.name.clone(),
                                ticket,
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        }),
                    );
                },
            );

            world.run(connection_manager_system);

            // Stream interface with collect() blocked forever
            let mut list = Vec::new();
            task::block_on(async {
                for _i in 0..5 {
                    let event = rx_channel.try_recv().unwrap();
                    list.push(event);
                }
            });

            if let Event::ResponseCheckVersion {
                connection_id,
                packet,
            } = &*list[0]
            {
                assert_eq!(*connection_id, con);
                assert_eq!(packet.ok, true);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Event::ResponseLoadingScreenControlInfo {
                connection_id,
                packet,
            } = &*list[1]
            {
                assert_eq!(*connection_id, con);
                assert_eq!(packet.custom_screen_enabled, false);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Event::ResponseRemainPlayTime {
                connection_id,
                packet,
            } = &*list[2]
            {
                assert_eq!(*connection_id, con);
                assert_eq!(packet.account_type, 6);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Event::ResponseLoginArbiter {
                connection_id,
                packet,
                account_id,
            } = &*list[3]
            {
                assert_eq!(*connection_id, con);
                assert_eq!(*account_id, account.id);
                assert_eq!(packet.success, true);
                assert_eq!(packet.status, 65538);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Event::ResponseLoginAccountInfo {
                connection_id,
                packet,
            } = &*list[4]
            {
                assert_eq!(*connection_id, con);
                assert_eq!(packet.account_id, account.id);
                assert_eq!(packet.server_name, "Almetica".to_string());
                assert!(!packet.server_name.trim().is_empty());
            } else {
                panic!("Received packets in wrong order");
            }

            Ok(())
        })
    }

    #[test]
    fn test_ping_pong_success() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (world, connection_id, rx_channel) = setup_with_connection(pool, true);

                // Set last pong so that we will get a PING event
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(PING_INTERVAL + 1))
                    .unwrap();

                world.run(|mut connections: ViewMut<Connection>| {
                    if let Ok(mut connection) = (&mut connections).try_get(connection_id) {
                        connection.last_pong = old_pong;
                    } else {
                        panic!("Couldn't find connection component");
                    }
                });

                world.run(connection_manager_system);

                if let Ok(event) = rx_channel.try_recv() {
                    match &*event {
                        Event::ResponsePing { .. } => { /* Ok */ }
                        _ => panic!("Didn't found the expected ping event."),
                    }
                } else {
                    panic!("Couldn't find ping event");
                }

                // Check if waiting_for_pong is updated
                world.run(|connections: View<Connection>| {
                    if let Ok(connection) = (&connections).try_get(connection_id) {
                        if !connection.waiting_for_pong {
                            panic!("Waiting_for_pong was not set after ping");
                        }
                    } else {
                        panic!("Couldn't find connection component");
                    }
                });

                // Send pong
                world.run(
                    |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                        entities.add_entity(
                            &mut events,
                            Box::new(Event::RequestPong {
                                connection_id,
                                packet: CPong {},
                            }),
                        )
                    },
                );

                world.run(connection_manager_system);

                // Check if last_pong is updated
                world.run(|connections: View<Connection>| {
                    let component = &connections[connection_id];
                    assert_eq!(component.last_pong > old_pong, true);
                });

                Ok(())
            })
        })
    }

    #[test]
    fn test_ping_pong_failure() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;

                let (world, connection_id, rx_channel) = setup_with_connection(pool, true);

                // Set last_pong in "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(PONG_DEADLINE + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<Connection>| {
                    connections[connection_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Check if drop connection event is present
                if let Ok(event) = rx_channel.try_recv() {
                    match &*event {
                        Event::ResponseDropConnection { .. } => { /* Ok */ }
                        _ => panic!(
                            "Couldn't find drop connection event. Found another packet instead."
                        ),
                    }
                } else {
                    panic!("Couldn't find drop connection event");
                }

                // Check if connection component was deleted
                assert!(world
                    .borrow::<View<Connection>>()
                    .try_get(connection_id)
                    .is_err());

                Ok(())
            })
        })
    }

    #[test]
    fn test_drop_unauthenticated_connection() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;

                let (world, connection_id, rx_channel) = setup_with_connection(pool, false);

                // Set last pong in "still ok" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME - 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<Connection>| {
                    connections[connection_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Connection should still be alive
                assert!(world
                    .borrow::<View<Connection>>()
                    .try_get(connection_id)
                    .is_ok());

                // Set last pong to "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<Connection>| {
                    connections[connection_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Check if drop connection event is present
                if let Ok(event) = rx_channel.try_recv() {
                    match &*event {
                        Event::ResponseDropConnection { .. } => { /* Ok */ }
                        _ => panic!(
                            "Couldn't find drop connection event. Found another packet instead."
                        ),
                    }
                } else {
                    panic!("Couldn't find drop connection event");
                }

                // Connection should be deleted
                assert!(world
                    .borrow::<View<Connection>>()
                    .try_get(connection_id)
                    .is_err());

                Ok(())
            })
        })
    }

    #[test]
    fn test_dont_drop_authenticated_connection_without_ping_ping() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;

                let (world, connection_id, _rx_channel) = setup_with_connection(pool, true);

                // Set last pong to "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<Connection>| {
                    connections[connection_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Connection should still be alive
                assert!(world
                    .borrow::<View<Connection>>()
                    .try_get(connection_id)
                    .is_ok());

                Ok(())
            })
        })
    }
}
