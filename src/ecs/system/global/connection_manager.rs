use crate::ecs::component::{Account, GlobalConnection, GlobalUserSpawn};
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::system::global::send_message_to_connection;
use crate::ecs::system::send_message;
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
const PING_INTERVAL: u64 = 15;
const PONG_DEADLINE: u64 = 30;

/// Connection manager handles the connection components.
pub fn connection_manager_system(
    incoming_messages: View<EcsMessage>,
    mut accounts: ViewMut<Account>,
    mut user_spawns: ViewMut<GlobalUserSpawn>,
    mut connections: ViewMut<GlobalConnection>,
    mut entities: EntitiesViewMut,
    pool: UniqueView<PgPool>,
) {
    // Incoming messages
    (&incoming_messages)
        .iter()
        .for_each(|message| match &**message {
            Message::RegisterConnection {
                connection_channel, ..
            } => {
                handle_connection_registration(
                    connection_channel.clone(),
                    &mut connections,
                    &mut entities,
                );
            }
            Message::RequestCheckVersion {
                connection_global_world_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_request_check_version(
                    *connection_global_world_id,
                    &packet,
                    &mut connections,
                ) {
                    error!("Rejecting Message::RequestCheckVersion: {:?}", e);
                    send_message_to_connection(
                        reject_check_version(*connection_global_world_id),
                        &connections,
                    );
                    drop_connection(
                        *connection_global_world_id,
                        &mut connections,
                        &mut user_spawns,
                    );
                }
            }
            Message::RequestLoginArbiter {
                connection_global_world_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_request_login_arbiter(
                    *connection_global_world_id,
                    &packet,
                    &mut accounts,
                    &mut connections,
                    &mut entities,
                    &pool,
                ) {
                    error!("Rejecting Message::RequestLoginArbiter: {:?}", e);
                    send_message_to_connection(
                        reject_login_arbiter(*connection_global_world_id, -1, packet.region),
                        &connections,
                    );
                    drop_connection(
                        *connection_global_world_id,
                        &mut connections,
                        &mut user_spawns,
                    );
                }
            }
            Message::RequestPong {
                connection_global_world_id,
                ..
            } => {
                id_span!(connection_global_world_id);
                handle_pong(*connection_global_world_id, &mut connections);
            }
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
        .for_each(|(connection_global_world_id, mut connection)| {
            id_span!(connection_global_world_id);
            if handle_ping(&now, connection_global_world_id, &mut connection) {
                // TODO set the "Logout" component to signal other systems to gracefully logout the user. Stuff like: close all transactions and signalling the local world to delete the user and send it's data to persist.
                to_drop.push(connection_global_world_id);
            }
        });

    // Unauthenticated connections only live for 5 seconds
    (&mut connections)
        .iter()
        .with_id()
        .filter(|(_, connection)| !connection.is_authenticated)
        .for_each(|(connection_global_world_id, connection)| {
            let last_pong_duration = now.duration_since(connection.last_pong).as_secs();
            if last_pong_duration >= MAX_UNAUTHENTICATED_LIFETIME {
                to_drop.push(connection_global_world_id);
            }
        });

    for connection_global_world_id in to_drop {
        id_span!(connection_global_world_id);
        drop_connection(
            connection_global_world_id,
            &mut connections,
            &mut user_spawns,
        );
    }
}

fn handle_connection_registration(
    connection_channel: Sender<EcsMessage>,
    connections: &mut ViewMut<GlobalConnection>,
    entities: &mut EntitiesViewMut,
) {
    debug!("Message::RegisterConnection incoming");

    // Create a new connection component to properly handle it's state
    let connection_global_world_id = entities.add_entity(
        &mut *connections,
        GlobalConnection {
            channel: connection_channel,
            is_authenticated: false,
            is_version_checked: false,
            last_pong: Instant::now(),
            waiting_for_pong: false,
        },
    );

    // Since we just created the component, we are sure to not panic here.
    let connection = connections.try_get(connection_global_world_id).unwrap();

    debug!("Registered connection as {:?}", connection_global_world_id);
    send_message(
        assemble_connection_registration_finished(connection_global_world_id),
        &connection.channel,
    );
}

fn handle_request_check_version(
    connection_global_world_id: EntityId,
    packet: &CCheckVersion,
    mut connections: &mut ViewMut<GlobalConnection>,
) -> Result<()> {
    debug!("Message::RequestCheckVersion incoming");

    ensure!(
        packet.version.len() == 2,
        format!(
            "Expected version array to be of length 2 but is {}",
            packet.version.len()
        )
    );

    debug!(
        "Version 1: {} version 2: {}",
        packet.version[0].value, packet.version[1].value
    );

    let mut connection = (&mut connections)
        .try_get(connection_global_world_id)
        .context("Could not find connection component for entity")?;
    connection.is_version_checked = true;

    Ok(())
}

fn handle_request_login_arbiter(
    connection_global_world_id: EntityId,
    packet: &CLoginArbiter,
    accounts: &mut ViewMut<Account>,
    mut connections: &mut ViewMut<GlobalConnection>,
    entities: &mut EntitiesViewMut,
    pool: &PgPool,
) -> Result<()> {
    debug!(
        "Message::RequestLoginArbiter incoming for account: {}",
        packet.master_account_name
    );

    Ok(task::block_on(async {
        let mut connection = (&mut connections)
            .try_get(connection_global_world_id)
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
        entities.add_component(accounts, account, connection_global_world_id);

        check_and_handle_post_initialization(connection_global_world_id, account, connection);

        Ok(())
    })?)
}

// Returns true if connection didn't return a ping in time.
fn handle_ping(
    now: &Instant,
    connection_global_world_id: EntityId,
    mut connection: &mut GlobalConnection,
) -> bool {
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
        send_message(
            assemble_ping(connection_global_world_id),
            &connection.channel,
        );
        false
    } else {
        false
    }
}

fn handle_pong(
    connection_global_world_id: EntityId,
    mut connections: &mut ViewMut<GlobalConnection>,
) {
    debug!("Message::RequestPong incoming");

    let span = info_span!("id", connection_global_world_id = ?connection_global_world_id);
    let _enter = span.enter();

    if let Ok(mut connection) = (&mut connections).try_get(connection_global_world_id) {
        connection.last_pong = Instant::now();
        connection.waiting_for_pong = false;
    } else {
        error!("Could not find connection component for entity");
    }
}

fn drop_connection(
    connection_global_world_id: EntityId,
    connections: &mut ViewMut<GlobalConnection>,
    user_spawns: &mut ViewMut<GlobalUserSpawn>,
) {
    if let Ok(connection) = connections.try_get(connection_global_world_id) {
        send_message(
            assemble_drop_connection(connection_global_world_id),
            &connection.channel,
        );
        connections.delete(connection_global_world_id);

        // TODO test the "marked_for_deletion" on spawned users
        if let Ok(spawn) = user_spawns.try_get(connection_global_world_id) {
            spawn.marked_for_deletion = true
        }
    } else {
        error!(
            "Couldn't find the connection component with the ID {:#?}",
            connection_global_world_id
        );
    }
}

fn check_and_handle_post_initialization(
    connection_global_world_id: EntityId,
    account: Account,
    connection: &GlobalConnection,
) {
    // Now that the client is vetted, we need to send him some specific packets in order for him to progress.
    debug!("Sending connection post initialization commands");

    // FIXME get from configuration (server name and PVP setting)!
    send_message(
        accept_check_version(connection_global_world_id),
        &connection.channel,
    );
    send_message(
        assemble_loading_screen_info(connection_global_world_id),
        &connection.channel,
    );
    send_message(
        assemble_remain_play_time(connection_global_world_id),
        &connection.channel,
    );
    send_message(
        accept_login_arbiter(connection_global_world_id, account.id, account.region),
        &connection.channel,
    );
    send_message(
        assemble_login_account_info(
            connection_global_world_id,
            "Almetica".to_string(),
            account.id,
        ),
        &connection.channel,
    );
}

fn assemble_loading_screen_info(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::ResponseLoadingScreenControlInfo {
        connection_global_world_id,
        packet: SLoadingScreenControlInfo {
            custom_screen_enabled: false,
        },
    })
}

fn assemble_remain_play_time(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::ResponseRemainPlayTime {
        connection_global_world_id,
        packet: SRemainPlayTime {
            account_type: 6,
            minutes_left: 0,
        },
    })
}

fn assemble_login_account_info(
    connection_global_world_id: EntityId,
    server_name: String,
    account_id: i64,
) -> EcsMessage {
    Box::new(Message::ResponseLoginAccountInfo {
        connection_global_world_id,
        packet: SLoginAccountInfo {
            server_name,
            account_id,
            integrity_iv: 0x0, // We don't care for the integrity hash, since it's broken anyhow.
        },
    })
}

fn assemble_ping(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::ResponsePing {
        connection_global_world_id,
        packet: SPing {},
    })
}

fn assemble_drop_connection(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::DropConnection {
        connection_global_world_id,
    })
}

fn assemble_connection_registration_finished(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::RegisterConnectionFinished {
        connection_global_world_id,
    })
}

fn accept_check_version(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::ResponseCheckVersion {
        connection_global_world_id,
        packet: SCheckVersion { ok: true },
    })
}

fn reject_check_version(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(Message::ResponseCheckVersion {
        connection_global_world_id,
        packet: SCheckVersion { ok: false },
    })
}

// TODO read PVP option out of configuration
fn accept_login_arbiter(
    connection_global_world_id: EntityId,
    account_id: i64,
    region: model::Region,
) -> EcsMessage {
    Box::new(Message::ResponseLoginArbiter {
        connection_global_world_id,
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
    connection_global_world_id: EntityId,
    account_id: i64,
    region: model::Region,
) -> EcsMessage {
    Box::new(Message::ResponseLoginArbiter {
        connection_global_world_id,
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
    use crate::ecs::message::Message;
    use crate::ecs::resource::DeletionList;
    use crate::ecs::system::common::cleaner_system;
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
        world.add_unique(DeletionList(vec![]));
        world.add_unique(pool);
        world
    }

    fn setup_with_connection(
        pool: PgPool,
        is_authenticated: bool,
    ) -> (World, EntityId, Receiver<EcsMessage>) {
        let world = World::new();
        world.add_unique(pool);

        let (tx_channel, rx_channel) = channel(1024);

        let connection_global_world_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<GlobalConnection>| {
                entities.add_entity(
                    &mut connections,
                    GlobalConnection {
                        channel: tx_channel,
                        is_authenticated,
                        is_version_checked: is_authenticated,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        (world, connection_global_world_id, rx_channel)
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
                    |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                        for _i in 0..5 {
                            entities.add_entity(
                                &mut messages,
                                Box::new(Message::RegisterConnection {
                                    connection_channel: tx_channel.clone(),
                                }),
                            );
                        }
                    },
                );

                world.run(connection_manager_system);

                let mut count = 0;
                loop {
                    if let Ok(message) = rx_channel.try_recv() {
                        match *message {
                            Message::RegisterConnectionFinished { .. } => count += 1,
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
                let (world, connection_global_world_id, _rx_channel) =
                    setup_with_connection(pool, true);

                world.run(
                    |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestCheckVersion {
                                connection_global_world_id,
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
                    .borrow::<View<GlobalConnection>>()
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
                let (world, connection_global_world_id, mut rx_channel) =
                    setup_with_connection(pool, true);

                world.run(
                    |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestCheckVersion {
                                connection_global_world_id,
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
                        .all(|message| match *message {
                            Message::ResponseCheckVersion { packet, .. } => !packet.ok,
                            Message::DropConnection { .. } => true,
                            _ => false,
                        })
                        .await,
                );

                // The connection should be dropped.
                let count = world.borrow::<View<GlobalConnection>>().iter().count();
                assert_eq!(count, 0);

                Ok(())
            })
        })
    }

    #[test]
    fn test_login_arbiter_valid() -> Result<()> {
        db_test(|db_string| {
            let (_conn, _rx_channel, world, connection_global_world_id, account, ticket) =
                task::block_on(async {
                    let pool = PgPool::new(db_string).await?;
                    let mut conn = pool.acquire().await?;
                    let (world, connection_global_world_id, rx_channel) =
                        setup_with_connection(pool, true);
                    let (account, ticket) = create_login(&mut conn).await?;

                    Ok::<
                        (
                            PoolConnection<PgConnection>,
                            Receiver<EcsMessage>,
                            World,
                            EntityId,
                            entity::Account,
                            Vec<u8>,
                        ),
                        anyhow::Error,
                    >((
                        conn,
                        rx_channel,
                        world,
                        connection_global_world_id,
                        account,
                        ticket,
                    ))
                })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestLoginArbiter {
                            connection_global_world_id,
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
            let (world, connection_global_world_id, rx_channel) = setup_with_connection(pool, true);
            let (account, mut ticket) = task::block_on(async { create_login(&mut conn).await })?;

            // Make ticket invalid
            ticket.make_ascii_uppercase();

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestLoginArbiter {
                            connection_global_world_id,
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
                if let Ok(message) = rx_channel.try_recv() {
                    match *message {
                        Message::ResponseLoginArbiter { packet, .. } => {
                            if !packet.success {
                                count += 1;
                            }
                        }
                        Message::DropConnection { .. } => {
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
            let count = world.borrow::<View<GlobalConnection>>().iter().count();
            assert_eq!(count, 0);

            Ok(())
        })
    }

    #[test]
    fn test_login_arbiter_reject_double_login() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel) = setup_with_connection(pool, true);
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
                        connection_global_world_id,
                    )
                },
            );

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestLoginArbiter {
                            connection_global_world_id,
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
                if let Ok(message) = rx_channel.try_recv() {
                    match *message {
                        Message::ResponseLoginArbiter { packet, .. } => {
                            if !packet.success {
                                count += 1;
                            }
                        }
                        Message::DropConnection { .. } => {
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
            let count = world.borrow::<View<GlobalConnection>>().iter().count();
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
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RegisterConnection {
                            connection_channel: tx_channel.clone(),
                        }),
                    )
                },
            );

            world.run(connection_manager_system);

            let con = match rx_channel.try_recv() {
                Ok(message) => match *message {
                    Message::RegisterConnectionFinished {
                        connection_global_world_id,
                    } => connection_global_world_id,
                    _ => panic!("Received wrong message"),
                },
                _ => panic!("Couldn't find message"),
            };

            // Run the cleaner to clean up all messages.
            world.run(cleaner_system);

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestCheckVersion {
                            connection_global_world_id: con,
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
                        &mut messages,
                        Box::new(Message::RequestLoginArbiter {
                            connection_global_world_id: con,
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
                    let message = rx_channel.try_recv().unwrap();
                    list.push(message);
                }
            });

            if let Message::ResponseCheckVersion {
                connection_global_world_id,
                packet,
            } = &*list[0]
            {
                assert_eq!(*connection_global_world_id, con);
                assert_eq!(packet.ok, true);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Message::ResponseLoadingScreenControlInfo {
                connection_global_world_id,
                packet,
            } = &*list[1]
            {
                assert_eq!(*connection_global_world_id, con);
                assert_eq!(packet.custom_screen_enabled, false);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Message::ResponseRemainPlayTime {
                connection_global_world_id,
                packet,
            } = &*list[2]
            {
                assert_eq!(*connection_global_world_id, con);
                assert_eq!(packet.account_type, 6);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Message::ResponseLoginArbiter {
                connection_global_world_id,
                packet,
                account_id,
            } = &*list[3]
            {
                assert_eq!(*connection_global_world_id, con);
                assert_eq!(*account_id, account.id);
                assert_eq!(packet.success, true);
                assert_eq!(packet.status, 65538);
            } else {
                panic!("Received packets in wrong order");
            }

            if let Message::ResponseLoginAccountInfo {
                connection_global_world_id,
                packet,
            } = &*list[4]
            {
                assert_eq!(*connection_global_world_id, con);
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
                let (world, connection_global_world_id, rx_channel) =
                    setup_with_connection(pool, true);

                // Set last pong so that we will get a PING message
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(PING_INTERVAL + 1))
                    .unwrap();

                world.run(|mut connections: ViewMut<GlobalConnection>| {
                    if let Ok(mut connection) =
                        (&mut connections).try_get(connection_global_world_id)
                    {
                        connection.last_pong = old_pong;
                    } else {
                        panic!("Couldn't find connection component");
                    }
                });

                world.run(connection_manager_system);

                if let Ok(message) = rx_channel.try_recv() {
                    match &*message {
                        Message::ResponsePing { .. } => { /* Ok */ }
                        _ => panic!("Didn't found the expected ping message."),
                    }
                } else {
                    panic!("Couldn't find ping message");
                }

                // Check if waiting_for_pong is updated
                world.run(|connections: View<GlobalConnection>| {
                    if let Ok(connection) = (&connections).try_get(connection_global_world_id) {
                        if !connection.waiting_for_pong {
                            panic!("Waiting_for_pong was not set after ping");
                        }
                    } else {
                        panic!("Couldn't find connection component");
                    }
                });

                // Send pong
                world.run(
                    |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestPong {
                                connection_global_world_id,
                                packet: CPong {},
                            }),
                        )
                    },
                );

                world.run(connection_manager_system);

                // Check if last_pong is updated
                world.run(|connections: View<GlobalConnection>| {
                    let component = &connections[connection_global_world_id];
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

                let (world, connection_global_world_id, rx_channel) =
                    setup_with_connection(pool, true);

                // Set last_pong in "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(PONG_DEADLINE + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<GlobalConnection>| {
                    connections[connection_global_world_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Check if drop connection message is present
                if let Ok(message) = rx_channel.try_recv() {
                    match &*message {
                        Message::DropConnection { .. } => { /* Ok */ }
                        _ => panic!(
                            "Couldn't find drop connection message. Found another packet instead."
                        ),
                    }
                } else {
                    panic!("Couldn't find drop connection message");
                }

                // Check if connection component was deleted
                assert!(world
                    .borrow::<View<GlobalConnection>>()
                    .try_get(connection_global_world_id)
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

                let (world, connection_global_world_id, rx_channel) =
                    setup_with_connection(pool, false);

                // Set last pong in "still ok" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME - 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<GlobalConnection>| {
                    connections[connection_global_world_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Connection should still be alive
                assert!(world
                    .borrow::<View<GlobalConnection>>()
                    .try_get(connection_global_world_id)
                    .is_ok());

                // Set last pong to "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<GlobalConnection>| {
                    connections[connection_global_world_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Check if drop connection message is present
                if let Ok(message) = rx_channel.try_recv() {
                    match &*message {
                        Message::DropConnection { .. } => { /* Ok */ }
                        _ => panic!(
                            "Couldn't find drop connection message. Found another packet instead."
                        ),
                    }
                } else {
                    panic!("Couldn't find drop connection message");
                }

                // Connection should be deleted
                assert!(world
                    .borrow::<View<GlobalConnection>>()
                    .try_get(connection_global_world_id)
                    .is_err());

                Ok(())
            })
        })
    }

    #[test]
    fn test_dont_drop_authenticated_connection_without_ping_pong() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;

                let (world, connection_global_world_id, _rx_channel) =
                    setup_with_connection(pool, true);

                // Set last pong to "getting dropped" range
                let now = Instant::now();
                let old_pong = now
                    .checked_sub(Duration::from_secs(MAX_UNAUTHENTICATED_LIFETIME + 1))
                    .unwrap();
                world.run(|mut connections: ViewMut<GlobalConnection>| {
                    connections[connection_global_world_id].last_pong = old_pong;
                });

                world.run(connection_manager_system);

                // Connection should still be alive
                assert!(world
                    .borrow::<View<GlobalConnection>>()
                    .try_get(connection_global_world_id)
                    .is_ok());

                Ok(())
            })
        })
    }
}
