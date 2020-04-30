use std::str::from_utf8;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, ensure, Context};
use async_std::sync::Sender;
use async_std::task;
use shipyard::*;
use sqlx::PgPool;
use tracing::{debug, error, info, info_span, trace};

use crate::ecs::component::{Connection, IncomingEvent, OutgoingEvent};
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::{ConnectionMapping, DeletionList, WorldId};
use crate::ecs::system::send_event;
use crate::model::repository::loginticket;
use crate::model::Region;
use crate::protocol::packet::*;
use crate::Result;

/// Connection manager handles the connection components.
pub fn connection_manager_system(
    incoming_events: View<IncomingEvent>,
    mut outgoing_events: ViewMut<OutgoingEvent>,
    mut connections: ViewMut<Connection>,
    mut entities: EntitiesViewMut,
    mut connection_map: UniqueViewMut<ConnectionMapping>,
    pool: UniqueView<PgPool>,
    mut deletion_list: UniqueViewMut<DeletionList>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    // Incoming events
    (&incoming_events).iter().for_each(|event| match &*event.0 {
        Event::RequestRegisterConnection {
            response_channel, ..
        } => handle_connection_registration(
            &response_channel,
            &mut connections,
            &mut outgoing_events,
            &mut entities,
            &mut connection_map,
        ),
        Event::RequestCheckVersion {
            connection_id,
            packet,
        } => {
            if let Err(e) = handle_request_check_version(
                *connection_id,
                &packet,
                &mut connections,
                &mut outgoing_events,
                &mut entities,
            ) {
                error!("Rejecting request check version event: {:?}", e);
                send_event(
                    reject_check_version(*connection_id),
                    &mut outgoing_events,
                    &mut entities,
                );
                drop_connection(
                    *connection_id,
                    &mut outgoing_events,
                    &mut entities,
                    &mut deletion_list,
                );
            }
        }
        Event::RequestLoginArbiter {
            connection_id,
            packet,
        } => {
            if let Err(e) = handle_request_login_arbiter(
                *connection_id,
                &packet,
                &mut connections,
                &pool,
                &mut outgoing_events,
                &mut entities,
            ) {
                error!("Rejecting login arbiter event: {:?}", e);
                send_event(
                    reject_login_arbiter(*connection_id, packet.region),
                    &mut outgoing_events,
                    &mut entities,
                );
                drop_connection(
                    *connection_id,
                    &mut outgoing_events,
                    &mut entities,
                    &mut deletion_list,
                );
            }
        }
        Event::RequestPong { connection_id, .. } => handle_pong(*connection_id, &mut connections),
        _ => { /* Ignore all other packets */ }
    });

    // Connections
    let now = Instant::now();
    (&mut connections)
        .iter()
        .with_id()
        .for_each(|(connection_id, mut connection)| {
            if handle_ping(
                &now,
                connection_id,
                &mut connection,
                &mut outgoing_events,
                &mut entities,
            ) {
                drop_connection(
                    connection_id,
                    &mut outgoing_events,
                    &mut entities,
                    &mut deletion_list,
                );
            }
        });
}

fn handle_connection_registration(
    response_channel: &Sender<EcsEvent>,
    connections: &mut ViewMut<Connection>,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
    connection_map: &mut UniqueViewMut<ConnectionMapping>,
) {
    debug!("Registration event incoming");

    // Create a new connection component to properly handle it's state
    let connection_id = entities.add_entity(
        connections,
        Connection {
            verified: false,
            version_checked: false,
            region: None,
            last_pong: Instant::now(),
            waiting_for_pong: false,
        },
    );

    // Create mapping so that the event sender knows which response channel to use.
    connection_map
        .0
        .insert(connection_id, response_channel.clone());

    debug!("Registered connection as {:?}", connection_id);

    send_event(
        accept_connection_registration(connection_id),
        outgoing_events,
        entities,
    );
}

fn handle_request_check_version(
    connection_id: EntityId,
    packet: &CCheckVersion,
    mut connections: &mut ViewMut<Connection>,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
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
    connection.version_checked = true;

    check_and_handle_post_initialization(connection_id, connection, outgoing_events, entities);

    Ok(())
}

fn handle_request_login_arbiter(
    connection_id: EntityId,
    packet: &CLoginArbiter,
    mut connections: &mut ViewMut<Connection>,
    pool: &PgPool,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!(
        "Login arbiter event incoming for account: {}",
        packet.master_account_name
    );

    Ok(task::block_on(async {
        let ticket = from_utf8(&packet.ticket).context("Ticket is not a valid UTF-8 string")?;
        trace!("Ticket value: {}", ticket);

        if ticket.trim().is_empty() {
            return Err(anyhow!("Ticket was empty"));
        }

        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if !loginticket::is_ticket_valid(&mut conn, &packet.master_account_name, &ticket)
            .await
            .context("Error while executing query for account")?
        {
            return Err(anyhow!("Ticket not valid"));
        }

        info!(
            "Account {} provided a valid ticket {}",
            packet.master_account_name, ticket
        );

        let mut connection = (&mut connections)
            .try_get(connection_id)
            .context("Could not find connection component for entity")?;

        connection.verified = true;
        connection.region = Some(packet.region);

        check_and_handle_post_initialization(connection_id, connection, outgoing_events, entities);

        Ok(())
    })?)
}

// Returns true if connection didn't return a ping in time.
fn handle_ping(
    now: &Instant,
    connection_id: EntityId,
    mut connection: &mut Connection,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
) -> bool {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    let last_pong_duration = now.duration_since(connection.last_pong).as_secs();

    if last_pong_duration >= 90 {
        debug!("Didn't received pong in 30 seconds. Dropping connection");
        true
    } else if !connection.waiting_for_pong && last_pong_duration >= 60 {
        debug!("Sending ping");
        connection.waiting_for_pong = true;
        send_event(assemble_ping(connection_id), outgoing_events, entities);
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

fn drop_connection(
    connection_id: EntityId,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
    deletion_list: &mut UniqueViewMut<DeletionList>,
) {
    send_event(
        assemble_drop_connection(connection_id),
        outgoing_events,
        entities,
    );
    deletion_list.0.push(connection_id);
}

fn check_and_handle_post_initialization(
    connection_id: EntityId,
    connection: &Connection,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
) {
    if connection.verified && connection.version_checked {
        if let Some(region) = connection.region {
            // Now that the client is vetted, we need to send him some specific packets in order for him to progress.
            debug!("Sending connection post initialization commands");

            // FIXME get from configuration (server name and PVP setting) and database (account_id). Set the account_id in the connection component!
            send_event(
                accept_check_version(connection_id),
                outgoing_events,
                entities,
            );
            send_event(
                assemble_loading_screen_info(connection_id),
                outgoing_events,
                entities,
            );
            send_event(
                assemble_remain_play_time(connection_id),
                outgoing_events,
                entities,
            );
            send_event(
                accept_login_arbiter(connection_id, region),
                outgoing_events,
                entities,
            );
            send_event(
                assemble_login_account_info(connection_id, "Almetica".to_string(), 456_456),
                outgoing_events,
                entities,
            );
        } else {
            error!("Region was not set in connection component");
        }
    }
}

fn assemble_loading_screen_info(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoadingScreenControlInfo {
        connection_id,
        packet: SLoadingScreenControlInfo {
            custom_screen_enabled: false,
        },
    }))
}

fn assemble_remain_play_time(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseRemainPlayTime {
        connection_id,
        packet: SRemainPlayTime {
            account_type: 6,
            minutes_left: 0,
        },
    }))
}

fn assemble_login_account_info(
    connection_id: EntityId,
    server_name: String,
    account_id: i64,
) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoginAccountInfo {
        connection_id,
        packet: SLoginAccountInfo {
            server_name,
            account_id,
            integrity_iv: 0x00000000, // We don't care for the integrity hash, since it's broken anyhow.
        },
    }))
}

fn assemble_ping(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponsePing {
        connection_id,
        packet: SPing {},
    }))
}

fn assemble_drop_connection(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseDropConnection { connection_id }))
}

fn accept_connection_registration(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
        connection_id,
    }))
}

fn accept_check_version(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseCheckVersion {
        connection_id,
        packet: SCheckVersion { ok: true },
    }))
}

fn reject_check_version(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseCheckVersion {
        connection_id,
        packet: SCheckVersion { ok: false },
    }))
}

// TODO read PVP option out of configuration
fn accept_login_arbiter(connection_id: EntityId, region: Region) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoginArbiter {
        connection_id,
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
    }))
}

// TODO read PVP option out of configuration
fn reject_login_arbiter(connection_id: EntityId, region: Region) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoginArbiter {
        connection_id,
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
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use async_std::sync::channel;
    use chrono::{TimeZone, Utc};
    use shipyard::*;
    use sqlx::{PgConnection, PgPool};

    use crate::ecs::component::{IncomingEvent, OutgoingEvent};
    use crate::ecs::event::Event;
    use crate::ecs::system::cleaner_system;
    use crate::model::entity::Account;
    use crate::model::repository::account;
    use crate::model::repository::loginticket;
    use crate::model::tests::db_test;
    use crate::model::{PasswordHashAlgorithm, Region};
    use crate::protocol::packet::CCheckVersion;
    use crate::Result;

    use super::*;

    fn setup(pool: PgPool) -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));

        let map = HashMap::new();
        world.add_unique(ConnectionMapping(map));
        world.add_unique(pool);

        world
    }

    fn setup_with_connection(pool: PgPool) -> (World, EntityId) {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        let map = HashMap::new();
        world.add_unique(ConnectionMapping(map));
        world.add_unique(pool);

        (world, connection_id)
    }

    async fn create_login(conn: &mut PgConnection) -> Result<(String, String)> {
        let acc = account::create(
            conn,
            &Account {
                id: -1,
                name: "testuser".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?;
        let ticket = loginticket::upsert_ticket(conn, acc.id).await?;
        Ok((acc.name, ticket.ticket))
    }

    #[test]
    fn test_connection_registration() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let world = setup(pool);
            let (tx_channel, _rx_channel) = channel(10);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    for _i in 0..5 {
                        entities.add_entity(
                            &mut events,
                            IncomingEvent(Arc::new(Event::RequestRegisterConnection {
                                response_channel: tx_channel.clone(),
                            })),
                        );
                    }
                },
            );

            world.run(connection_manager_system);

            world.run(|events: ViewMut<OutgoingEvent>| {
                let count = (&events)
                    .iter()
                    .filter(|event| match &*event.0 {
                        Event::ResponseRegisterConnection { .. } => true,
                        _ => false,
                    })
                    .count();
                assert_eq!(count, 5);
            });

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_check_version_valid() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let (world, connection_id) = setup_with_connection(pool);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestCheckVersion {
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
                        })),
                    )
                },
            );

            world.run(connection_manager_system);

            let valid_count = world
                .borrow::<View<Connection>>()
                .iter()
                .filter(|connection| connection.version_checked)
                .count();
            assert_eq!(valid_count, 1);

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_check_version_invalid() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let (world, connection_id) = setup_with_connection(pool);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestCheckVersion {
                            connection_id,
                            packet: CCheckVersion {
                                version: vec![CCheckVersionEntry {
                                    index: 0,
                                    value: 366_222,
                                }],
                            },
                        })),
                    )
                },
            );

            world.run(connection_manager_system);

            let count = world
                .borrow::<View<OutgoingEvent>>()
                .iter()
                .filter(|event| match &*event.0 {
                    Event::ResponseCheckVersion { packet, .. } => !packet.ok,
                    Event::ResponseDropConnection { .. } => true,
                    _ => false,
                })
                .count();
            assert_eq!(count, 2);

            let invalid_count = world
                .borrow::<View<Connection>>()
                .iter()
                .filter(|connection| !connection.version_checked)
                .count();
            assert_eq!(invalid_count, 1);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_login_arbiter_valid() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await?;
            let (world, connection_id) = setup_with_connection(pool);
            let (account_name, ticket) = create_login(&mut conn).await?;

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestLoginArbiter {
                            connection_id,
                            packet: CLoginArbiter {
                                master_account_name: account_name,
                                ticket: ticket.as_bytes().to_vec(),
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        })),
                    )
                },
            );

            world.run(connection_manager_system);

            let valid_count = world
                .borrow::<View<Connection>>()
                .iter()
                .filter(|connection| connection.verified)
                .count();
            assert_eq!(valid_count, 1);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_login_arbiter_invalid() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await?;
            let (world, connection_id) = setup_with_connection(pool);
            let (account_name, mut ticket) = create_login(&mut conn).await?;

            // Make ticket invalid
            ticket.make_ascii_uppercase();

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestLoginArbiter {
                            connection_id,
                            packet: CLoginArbiter {
                                master_account_name: account_name,
                                ticket: ticket.as_bytes().to_vec(),
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        })),
                    )
                },
            );

            world.run(connection_manager_system);

            let count = world
                .borrow::<View<OutgoingEvent>>()
                .iter()
                .filter(|event| match &*event.0 {
                    Event::ResponseLoginArbiter { packet, .. } => !packet.success,
                    Event::ResponseDropConnection { .. } => true,
                    _ => false,
                })
                .count();
            assert_eq!(count, 2);

            let invalid_count = world
                .borrow::<View<Connection>>()
                .iter()
                .filter(|connection| !connection.verified)
                .count();
            assert_eq!(invalid_count, 1);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_login_sequence() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await?;
            let world = setup(pool);
            let (account_name, ticket) = create_login(&mut conn).await?;
            let (tx_channel, _rx_channel) = channel(10);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestRegisterConnection {
                            response_channel: tx_channel.clone(),
                        })),
                    )
                },
            );

            world.run(connection_manager_system);

            let con = world.run(|events: View<OutgoingEvent>| {
                if let Some(event) = (&events).iter().next() {
                    match *event.0 {
                        Event::ResponseRegisterConnection { connection_id } => connection_id,
                        _ => panic!("received wrong event"),
                    }
                } else {
                    panic!("couldn't find response register connection event");
                }
            });

            // Run the cleaner to clean up all events.
            world.run(cleaner_system);

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestCheckVersion {
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
                        })),
                    );
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestLoginArbiter {
                            connection_id: con,
                            packet: CLoginArbiter {
                                master_account_name: account_name,
                                ticket: ticket.as_bytes().to_vec(),
                                unk1: 0,
                                unk2: 0,
                                region: Region::Europe,
                                patch_version: 9002,
                            },
                        })),
                    );
                },
            );

            world.run(connection_manager_system);

            world.run(|events: View<OutgoingEvent>| {
                let list: Vec<&OutgoingEvent> = (&events).iter().collect();
                assert_eq!(list.len(), 5);

                if let Event::ResponseCheckVersion {
                    connection_id,
                    packet,
                } = &*list[0].0
                {
                    assert_eq!(*connection_id, con);
                    assert_eq!(packet.ok, true);
                } else {
                    panic!("received packets in wrong order");
                }

                if let Event::ResponseLoadingScreenControlInfo {
                    connection_id,
                    packet,
                } = &*list[1].0
                {
                    assert_eq!(*connection_id, con);
                    assert_eq!(packet.custom_screen_enabled, false);
                } else {
                    panic!("received packets in wrong order");
                }

                if let Event::ResponseRemainPlayTime {
                    connection_id,
                    packet,
                } = &*list[2].0
                {
                    assert_eq!(*connection_id, con);
                    assert_eq!(packet.account_type, 6);
                } else {
                    panic!("received packets in wrong order");
                }

                if let Event::ResponseLoginArbiter {
                    connection_id,
                    packet,
                } = &*list[3].0
                {
                    assert_eq!(*connection_id, con);
                    assert_eq!(packet.success, true);
                    assert_eq!(packet.status, 65538);
                } else {
                    panic!("received packets in wrong order");
                }

                if let Event::ResponseLoginAccountInfo {
                    connection_id,
                    packet,
                } = &*list[4].0
                {
                    assert_eq!(*connection_id, con);
                    assert!(!packet.server_name.trim().is_empty());
                } else {
                    panic!("received packets in wrong order");
                }
            });

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_ping_pong_success() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let (world, connection_id) = setup_with_connection(pool);

            // Set last pong 61 seconds ago.
            let now = Instant::now();
            let old_pong = now.checked_sub(Duration::from_secs(61)).unwrap();

            world.run(|mut connections: ViewMut<Connection>| {
                if let Ok(mut connection) = (&mut connections).try_get(connection_id) {
                    connection.last_pong = old_pong;
                } else {
                    panic!("Couldn't find connection component");
                }
            });

            world.run(connection_manager_system);

            let count = world.borrow::<View<OutgoingEvent>>().iter().count();
            assert_eq!(count, 1);

            // Check if ping is present
            let mut to_delete: Option<EntityId> = None;

            if let Some((entity, event)) = world
                .borrow::<View<OutgoingEvent>>()
                .iter()
                .with_id()
                .next()
            {
                match &*event.0 {
                    Event::ResponsePing { .. } => {
                        to_delete = Some(entity);
                    }
                    _ => panic!("Couldn't find ping event"),
                }
            }
            world
                .borrow::<AllStoragesViewMut>()
                .delete(to_delete.unwrap());

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
                |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestPong {
                            connection_id,
                            packet: CPong {},
                        })),
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
        }
        db_test(test)
    }

    #[test]
    fn test_ping_pong_failure() -> Result<()> {
        async fn test(pool: PgPool) -> Result<()> {
            let (world, connection_id) = setup_with_connection(pool);

            // Set last pong 91 seconds ago.
            let now = Instant::now();
            let old_pong = now.checked_sub(Duration::from_secs(91)).unwrap();
            world.run(|mut connections: ViewMut<Connection>| {
                connections[connection_id].last_pong = old_pong;
            });

            world.run(connection_manager_system);

            let count = world.borrow::<ViewMut<OutgoingEvent>>().iter().count();
            assert_eq!(count, 1);

            // Check if drop connection event is present
            world.run(|events: View<OutgoingEvent>| {
                if let Some(event) = (&events).iter().next() {
                    match &*event.0 {
                        Event::ResponseDropConnection { .. } => { /* do nothing */ }
                        _ => panic!("Couldn't find drop connection event"),
                    }
                }
            });

            // Run the cleaner so that the connection is cleaned up.
            world.run(cleaner_system);

            // Check if connection component was deleted
            if let Ok(_component) = world.borrow::<View<Connection>>().try_get(connection_id) {
                panic!("Found the connection component even though it should have been deleted");
            };

            Ok(())
        }
        db_test(test)
    }
}
