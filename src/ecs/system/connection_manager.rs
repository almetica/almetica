/// Connection handler handles the connection components.
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Instant;

use shipyard::prelude::*;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error, info_span, trace};

use crate::ecs::component::{Connection, IncomingEvent, OutgoingEvent};
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::{ConnectionMapping, DeletionList, WorldId};
use crate::ecs::system::send_event;
use crate::model::Region;
use crate::protocol::packet::*;

pub struct ConnectionManager;

impl<'sys> System<'sys> for ConnectionManager {
    type Data = (
        &'sys IncomingEvent,
        &'sys mut OutgoingEvent,
        &'sys mut Connection,
        EntitiesMut,
        Unique<&'sys mut ConnectionMapping>,
        Unique<&'sys mut DeletionList>,
        Unique<&'sys WorldId>,
    );

    fn run(
        (
            incoming_events,
            mut outgoing_events,
            mut connections,
            mut entities,
            mut connection_map,
            mut deletion_list,
            world_id,
        ): <Self::Data as SystemData<'sys>>::View,
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
            } => handle_request_check_version(
                *connection_id,
                &packet,
                &mut connections,
                &mut outgoing_events,
                &mut entities,
            ),
            Event::RequestLoginArbiter {
                connection_id,
                packet,
            } => handle_request_login_arbiter(
                *connection_id,
                &packet,
                &mut connections,
                &mut outgoing_events,
                &mut entities,
            ),
            Event::RequestPong { connection_id, .. } => {
                handle_pong(*connection_id, &mut connections)
            }
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
                    deletion_list.0.push(connection_id);
                }
            });
    }
}

fn handle_connection_registration(
    response_channel: &Sender<EcsEvent>,
    connections: &mut ViewMut<Connection>,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
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
    connection_id: Option<EntityId>,
    packet: &CCheckVersion,
    mut connections: &mut ViewMut<Connection>,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
) {
    if let Some(connection_id) = connection_id {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        debug!("Check version event incoming");

        if packet.version.len() != 2 {
            error!(
                "Expected version array to be of length 2 but is {}",
                packet.version.len()
            );
            send_event(
                reject_check_version(connection_id),
                outgoing_events,
                entities,
            );
            return;
        }

        // TODO properly do the version verification
        trace!(
            "Version 1: {} version 2: {}",
            packet.version[0].value,
            packet.version[1].value
        );

        if let Ok(mut connection) = (&mut connections).get(connection_id) {
            connection.version_checked = true;
            check_and_handle_post_initialization(
                connection_id,
                connection,
                outgoing_events,
                entities,
            );
        } else {
            error!("Could not find connection component for entity");
            send_event(
                reject_check_version(connection_id),
                outgoing_events,
                entities,
            );
        };
    } else {
        error!("Entity of the connection for check version event was not set");
    }
}

fn handle_request_login_arbiter(
    connection_id: Option<EntityId>,
    packet: &CLoginArbiter,
    mut connections: &mut ViewMut<Connection>,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
) {
    if let Some(connection_id) = connection_id {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        debug!(
            "Login arbiter event incoming for master account: {}",
            packet.master_account_name
        );

        if let Ok(ticket) = from_utf8(&packet.ticket) {
            trace!("Ticket value: {}", ticket);

            // TODO properly handle the request with DB and token verification
            if ticket.trim().is_empty() {
                error!("Ticket was empty. Rejecting");
                send_event(
                    reject_login_arbiter(connection_id, packet.region),
                    outgoing_events,
                    entities,
                );
                return;
            }

            if let Ok(mut connection) = (&mut connections).get(connection_id) {
                connection.verified = true;
                connection.region = Some(packet.region);
                check_and_handle_post_initialization(
                    connection_id,
                    connection,
                    outgoing_events,
                    entities,
                )
            } else {
                error!("Could not find connection component for entity. Rejecting");
                send_event(
                    reject_login_arbiter(connection_id, packet.region),
                    outgoing_events,
                    entities,
                );
            }
        } else {
            error!("Ticket is not a valid UTF-8 string");
        };
    } else {
        error!("Entity of the connection for login arbiter event was not set");
    }
}

// Returns true if connection didn't return a ping in time.
fn handle_ping(
    now: &Instant,
    connection_id: EntityId,
    mut connection: &mut Connection,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
) -> bool {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    let last_pong_duration = now.duration_since(connection.last_pong).as_secs();

    if last_pong_duration >= 90 {
        debug!("Didn't received pong in 30 seconds. Dropping connection");
        send_event(
            assemble_drop_connection(connection_id),
            outgoing_events,
            entities,
        );
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

fn handle_pong(connection_id: Option<EntityId>, mut connections: &mut ViewMut<Connection>) {
    if let Some(connection_id) = connection_id {
        debug!("Pong event incoming");

        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        if let Ok(mut connection) = (&mut connections).get(connection_id) {
            connection.last_pong = Instant::now();
            connection.waiting_for_pong = false;
        } else {
            error!("Could not find connection component for entity");
        }
    } else {
        error!("Entity of the connection for pong event was not set");
    }
}

fn check_and_handle_post_initialization(
    connection_id: EntityId,
    connection: &Connection,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
) {
    if connection.verified && connection.version_checked {
        if let Some(region) = connection.region {
            // Now that the client is vetted, we need to send him some specific packets in order for him to progress.
            debug!("Sending connection post initialization commands");

            // TODO get from configuration and database
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
        connection_id: Some(connection_id),
        packet: SLoadingScreenControlInfo {
            custom_screen_enabled: false,
        },
    }))
}

fn assemble_remain_play_time(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseRemainPlayTime {
        connection_id: Some(connection_id),
        packet: SRemainPlayTime {
            account_type: 6,
            minutes_left: 0,
        },
    }))
}

fn assemble_login_account_info(
    connection_id: EntityId,
    server_name: String,
    account_id: u64,
) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoginAccountInfo {
        connection_id: Some(connection_id),
        packet: SLoginAccountInfo {
            server_name,
            account_id,
        },
    }))
}

fn assemble_ping(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponsePing {
        connection_id: Some(connection_id),
        packet: SPing {},
    }))
}

fn assemble_drop_connection(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseDropConnection {
        connection_id: Some(connection_id),
    }))
}

fn accept_connection_registration(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
        connection_id: Some(connection_id),
    }))
}

fn accept_check_version(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseCheckVersion {
        connection_id: Some(connection_id),
        packet: SCheckVersion { ok: true },
    }))
}

fn reject_check_version(connection_id: EntityId) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseCheckVersion {
        connection_id: Some(connection_id),
        packet: SCheckVersion { ok: false },
    }))
}

// TODO read PVP option out of configuration
fn accept_login_arbiter(connection_id: EntityId, region: Region) -> OutgoingEvent {
    OutgoingEvent(Arc::new(Event::ResponseLoginArbiter {
        connection_id: Some(connection_id),
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
        connection_id: Some(connection_id),
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

    use shipyard::prelude::*;
    use tokio::sync::mpsc::channel;

    use crate::ecs::component::{IncomingEvent, OutgoingEvent};
    use crate::ecs::event::Event;
    use crate::ecs::system::Cleaner;
    use crate::model::Region;
    use crate::protocol::packet::CCheckVersion;

    use super::*;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));

        let map = HashMap::new();
        world.add_unique(ConnectionMapping(map));

        world
    }

    fn setup_with_connection() -> (World, EntityId) {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));

        let connection_id = world.run::<(EntitiesMut, &mut Connection), EntityId, _>(
            |(mut entities, mut connections)| {
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

        (world, connection_id)
    }

    #[test]
    fn test_connection_registration() {
        let world = setup();
        let (tx_channel, _rx_channel) = channel(10);

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            for _i in 0..5 {
                entities.add_entity(
                    &mut events,
                    IncomingEvent(Arc::new(Event::RequestRegisterConnection {
                        connection_id: None,
                        response_channel: tx_channel.clone(),
                    })),
                );
            }
        });

        world.run_system::<ConnectionManager>();

        world.run::<&mut OutgoingEvent, _, _>(|events| {
            let count = (&events)
                .iter()
                .filter(|event| match &*event.0 {
                    Event::ResponseRegisterConnection { .. } => true,
                    _ => false,
                })
                .count();
            assert_eq!(count, 5);
        });
    }

    #[test]
    fn test_check_version_valid() {
        let (world, connection_id) = setup_with_connection();

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestCheckVersion {
                    connection_id: Some(connection_id),
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
        });

        world.run_system::<ConnectionManager>();

        let valid_count = world
            .borrow::<&Connection>()
            .iter()
            .filter(|connection| connection.version_checked)
            .count();
        assert_eq!(valid_count, 1);
    }

    #[test]
    fn test_check_version_invalid() {
        let (world, connection_id) = setup_with_connection();

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestCheckVersion {
                    connection_id: Some(connection_id),
                    packet: CCheckVersion {
                        version: vec![CCheckVersionEntry {
                            index: 0,
                            value: 366_222,
                        }],
                    },
                })),
            )
        });

        world.run_system::<ConnectionManager>();

        let count = world
            .borrow::<&OutgoingEvent>()
            .iter()
            .filter(|event| match &*event.0 {
                Event::ResponseCheckVersion { packet, .. } => !packet.ok,
                _ => false,
            })
            .count();
        assert_eq!(count, 1);

        let invalid_count = world
            .borrow::<&Connection>()
            .iter()
            .filter(|connection| !connection.version_checked)
            .count();
        assert_eq!(invalid_count, 1);
    }

    #[test]
    fn test_login_arbiter_valid() {
        let (world, connection_id) = setup_with_connection();

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestLoginArbiter {
                    connection_id: Some(connection_id),
                    packet: CLoginArbiter {
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![
                            79, 83, 99, 71, 75, 116, 109, 114, 51, 115, 110, 103, 98, 52, 49, 56,
                            114, 70, 110, 72, 69, 68, 87, 77, 84, 114, 89, 83, 98, 72, 97, 50, 56,
                            48, 106, 118, 101, 90, 116, 67, 101, 71, 55, 84, 55, 112, 88, 118, 55,
                            72,
                        ],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                })),
            )
        });

        world.run_system::<ConnectionManager>();

        let valid_count = world
            .borrow::<&Connection>()
            .iter()
            .filter(|connection| connection.verified)
            .count();
        assert_eq!(valid_count, 1);
    }

    #[test]
    fn test_login_arbiter_invalid() {
        let (world, connection_id) = setup_with_connection();

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestLoginArbiter {
                    connection_id: Some(connection_id),
                    packet: CLoginArbiter {
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                })),
            )
        });

        world.run_system::<ConnectionManager>();

        let count = world
            .borrow::<&OutgoingEvent>()
            .iter()
            .filter(|event| match &*event.0 {
                Event::ResponseLoginArbiter { packet, .. } => !packet.success,
                _ => false,
            })
            .count();
        assert_eq!(count, 1);

        let valid_count = world
            .borrow::<&Connection>()
            .iter()
            .filter(|connection| !connection.verified)
            .count();
        assert_eq!(valid_count, 1);
    }

    #[test]
    fn test_login_sequence() {
        let world = setup();
        let (tx_channel, _rx_channel) = channel(10);

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestRegisterConnection {
                    connection_id: None,
                    response_channel: tx_channel.clone(),
                })),
            )
        });

        world.run_system::<ConnectionManager>();

        let con = world.run::<&OutgoingEvent, Option<EntityId>, _>(|events| {
            if let Some(event) = (&events).iter().next() {
                match *event.0 {
                    Event::ResponseRegisterConnection { connection_id } => connection_id,
                    _ => None,
                }
            } else {
                panic!("couldn't find response register connection event");
            }
        });
        assert_ne!(con, None);

        // Run the cleaner to clean up all events.
        world.run_system::<Cleaner>();

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
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
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![
                            79, 83, 99, 71, 75, 116, 109, 114, 51, 115, 110, 103, 98, 52, 49, 56,
                            114, 70, 110, 72, 69, 68, 87, 77, 84, 114, 89, 83, 98, 72, 97, 50, 56,
                            48, 106, 118, 101, 90, 116, 67, 101, 71, 55, 84, 55, 112, 88, 118, 55,
                            72,
                        ],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                })),
            );
        });

        world.run_system::<ConnectionManager>();

        world.run::<&OutgoingEvent, _, _>(|events| {
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
                assert_ne!(packet.server_name.trim().is_empty(), true);
            } else {
                panic!("received packets in wrong order");
            }
        });
    }

    #[test]
    fn test_ping_pong_success() {
        let (world, connection_id) = setup_with_connection();

        // Set last pong 61 seconds ago.
        let now = Instant::now();
        let old_pong = now.checked_sub(Duration::from_secs(61)).unwrap();

        world.run::<&mut Connection, _, _>(|mut connections| {
            if let Ok(mut connection) = (&mut connections).get(connection_id) {
                connection.last_pong = old_pong;
            } else {
                panic!("Couldn't find connection component");
            }
        });

        world.run_system::<ConnectionManager>();

        let count = world.borrow::<&OutgoingEvent>().iter().count();
        assert_eq!(count, 1);

        // Check if ping is present
        let mut to_delete: Option<EntityId> = None;

        if let Some((entity, event)) = world.borrow::<&OutgoingEvent>().iter().with_id().next() {
            match &*event.0 {
                Event::ResponsePing { .. } => {
                    to_delete = Some(entity);
                }
                _ => panic!("Couldn't find ping event"),
            }
        }
        world.borrow::<AllStorages>().delete(to_delete.unwrap());

        // Check if waiting_for_pong is updated
        world.run::<&Connection, _, _>(|connections| {
            if let Ok(connection) = (&connections).get(connection_id) {
                if !connection.waiting_for_pong {
                    panic!("Waiting_for_pong was not set after ping");
                }
            } else {
                panic!("Couldn't find connection component");
            }
        });

        // Send pong
        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            entities.add_entity(
                &mut events,
                IncomingEvent(Arc::new(Event::RequestPong {
                    connection_id: Some(connection_id),
                    packet: CPong {},
                })),
            )
        });

        world.run_system::<ConnectionManager>();

        // Check if last_pong is updated
        world.run::<&Connection, _, _>(|connections| {
            let component = &connections[connection_id];
            assert_eq!(component.last_pong > old_pong, true);
        });
    }

    #[test]
    fn test_ping_pong_failure() {
        let (world, connection_id) = setup_with_connection();

        // Set last pong 91 seconds ago.
        let now = Instant::now();
        let old_pong = now.checked_sub(Duration::from_secs(91)).unwrap();
        world.run::<&mut Connection, _, _>(|mut connections| {
            connections[connection_id].last_pong = old_pong;
        });

        world.run_system::<ConnectionManager>();

        let count = world.borrow::<&mut OutgoingEvent>().iter().count();
        assert_eq!(count, 1);

        // Check if drop connection event is present
        world.run::<&OutgoingEvent, _, _>(|events| {
            if let Some(event) = (&events).iter().next() {
                match &*event.0 {
                    Event::ResponseDropConnection { .. } => { /* do nothing */ }
                    _ => panic!("Couldn't find drop connection event"),
                }
            }
        });

        // Run the cleaner so that the connection is cleaned up.
        world.run_system::<Cleaner>();

        // Check if connection component was deleted
        if let Ok(_component) = world.borrow::<&Connection>().get(connection_id) {
            panic!("Found the connection component even though it should have been deleted");
        };
    }
}
