/// Connection handler handles the connection components.
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Instant;

use crate::ecs::component::{BatchEvent, Connection, SingleEvent};
use crate::ecs::event::Event;
use crate::ecs::event::EventKind;
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::system::send_event;
use crate::ecs::tag;
use crate::model::Region;
use crate::protocol::packet::*;
use crate::*;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::{SubWorld, SystemBuilder};
use tokio::sync::mpsc::Sender;
use tracing::{debug, error, info_span, trace};

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("ConnectionManager")
        .write_resource::<ConnectionMapping>()
        .with_query(<Read<SingleEvent>>::query().filter(tag_value(&tag::EventKind(EventKind::Request))))
        .with_query(<Read<Connection>>::query())
        .write_component::<SingleEvent>()
        .write_component::<BatchEvent>()
        .write_component::<Connection>()
        .build(move |mut command_buffer, mut world, connection_mapping, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            // SingleEvents
            for event in queries.0.iter_mut(&mut *world) {
                match &**event {
                    Event::RequestRegisterConnection { response_channel, .. } => {
                        handle_connection_registration(
                            &mut connection_mapping.map,
                            response_channel,
                            &mut command_buffer,
                        );
                    }
                    Event::RequestCheckVersion { connection, packet } => {
                        if let Err(e) =
                            handle_request_check_version(*connection, &packet, &mut world, &mut command_buffer)
                        {
                            debug!("Can't handle RequestCheckVersion event: {:?}", e);
                        }
                    }
                    Event::RequestLoginArbiter { connection, packet } => {
                        if let Err(e) =
                            handle_request_login_arbiter(*connection, &packet, &mut world, &mut command_buffer)
                        {
                            debug!("Can't handle RequestLoginArbiter event: {:?}", e);
                        }
                    }
                    // TODO handle PONG
                    _ => { /* Ignore all other events */ }
                }
            }

            // Connections
            let now = Instant::now();
            for connection in queries.1.iter_mut(&mut *world) {
                if now.duration_since(connection.last_pong).as_secs() >= 90 {
                    // TODO disconnect client if they don't response to a ping
                } else if now.duration_since(connection.last_pong).as_secs() >= 60 {
                    // TODO send ping
                }
            }
        })
}

fn handle_connection_registration(
    connection_mapping: &mut HashMap<Entity, Sender<SingleEvent>>,
    response_channel: &Sender<SingleEvent>,
    mut command_buffer: &mut CommandBuffer,
) {
    debug!("Registration event incoming");

    // Create a new connection component to properly handle it's state
    let connection = Connection {
        verified: false,
        version_checked: false,
        region: None,
        last_pong: Instant::now(),
    };
    let connection_entity = command_buffer.start_entity().with_component((connection,)).build();

    // Create mapping so that the event dispatcher knows which response channel to use.
    connection_mapping.insert(connection_entity, response_channel.clone());

    debug!("Registered connection with entity id {}", connection_entity.index());

    send_event(accept_connection_registration(connection_entity), &mut command_buffer);
}

fn handle_request_check_version(
    connection: Option<Entity>,
    packet: &CCheckVersion,
    world: &mut SubWorld,
    mut command_buffer: &mut CommandBuffer,
) -> Result<()> {
    if let Some(connection) = connection {
        let span = info_span!("connection", %connection);
        let _enter = span.enter();

        debug!("Check version event incoming");

        if packet.version.len() != 2 {
            error!(
                "Expected version array to be of length 2 but is {}",
                packet.version.len()
            );
            send_event(reject_check_version(connection), &mut command_buffer);
            return Ok(());
        }

        // TODO properly do the version verification

        trace!(
            "Version 1: {} version 2: {}",
            packet.version[0].value,
            packet.version[1].value
        );

        if let Some(mut component) = world.get_component_mut::<Connection>(connection) {
            component.version_checked = true;
            check_and_handle_post_initialization(connection, &component, &mut command_buffer);
        } else {
            error!("Could not find connection component for entity");
            send_event(reject_check_version(connection), &mut command_buffer);
        }
        Ok(())
    } else {
        error!("Entity of the connection for event RequestCheckVersion was not set");
        Err(Error::EntityNotSet)
    }
}

fn handle_request_login_arbiter(
    connection: Option<Entity>,
    packet: &CLoginArbiter,
    world: &mut SubWorld,
    mut command_buffer: &mut CommandBuffer,
) -> Result<()> {
    if let Some(connection) = connection {
        let span = info_span!("connection", %connection);
        let _enter = span.enter();

        debug!(
            "Login arbiter event incoming for master account: {}",
            packet.master_account_name
        );
        let ticket = from_utf8(&packet.ticket)?;
        trace!("Ticket value: {}", ticket);

        // TODO properly handle the request with DB and token verification
        if ticket.trim().is_empty() {
            error!("Ticket was empty. Rejecting");
            send_event(reject_login_arbiter(connection, packet.region), &mut command_buffer);
        }

        if let Some(mut component) = world.get_component_mut::<Connection>(connection) {
            component.verified = true;
            component.region = Some(packet.region);
            check_and_handle_post_initialization(connection, &component, &mut command_buffer);
        } else {
            error!("Could not find connection component for entity. Rejecting");
            send_event(reject_login_arbiter(connection, packet.region), &mut command_buffer);
        }
        Ok(())
    } else {
        error!("Entity of the connection for event RequestCheckVersion was not set");
        Err(Error::EntityNotSet)
    }
}

fn check_and_handle_post_initialization(
    connection: Entity,
    component: &Connection,
    mut command_buffer: &mut CommandBuffer,
) {
    if component.verified && component.version_checked {
        if let Some(region) = component.region {
            // Now that the client is vetted, we need to send him some specific
            // packets in order for him to progress.
            debug!("Sending connection post initialization commands");

            // TODO get from configuration and database
            let batch = vec![
                accept_check_version(connection),
                assemble_loading_screen_info(connection),
                assemble_remain_play_time(connection),
                accept_login_arbiter(connection, region),
                assemble_login_account_info(connection, "Almetica".to_string(), 456_456),
            ];
            send_batch_event(batch, &mut command_buffer);
        } else {
            error!("Region was not set in connection component");
        }
    }
}

fn send_batch_event(batch: BatchEvent, command_buffer: &mut CommandBuffer) {
    debug!("Created batch event with {} events", batch.len());
    command_buffer
        .start_entity()
        .with_tag((tag::EventKind(EventKind::Response),))
        .with_component((batch,))
        .build();
}

fn assemble_loading_screen_info(connection: Entity) -> SingleEvent {
    Arc::new(Event::ResponseLoadingScreenControlInfo {
        connection: Some(connection),
        packet: SLoadingScreenControlInfo {
            custom_screen_enabled: false,
        },
    })
}

fn assemble_remain_play_time(connection: Entity) -> SingleEvent {
    Arc::new(Event::ResponseRemainPlayTime {
        connection: Some(connection),
        packet: SRemainPlayTime {
            account_type: 6,
            minutes_left: 0,
        },
    })
}

fn assemble_login_account_info(connection: Entity, server_name: String, account_id: u64) -> SingleEvent {
    Arc::new(Event::ResponseLoginAccountInfo {
        connection: Some(connection),
        packet: SLoginAccountInfo {
            server_name,
            account_id,
        },
    })
}

fn accept_connection_registration(connection: Entity) -> SingleEvent {
    Arc::new(Event::ResponseRegisterConnection {
        connection: Some(connection),
    })
}

fn accept_check_version(connection: Entity) -> SingleEvent {
    Arc::new(Event::ResponseCheckVersion {
        connection: Some(connection),
        packet: SCheckVersion { ok: true },
    })
}

fn reject_check_version(connection: Entity) -> SingleEvent {
    Arc::new(Event::ResponseCheckVersion {
        connection: Some(connection),
        packet: SCheckVersion { ok: false },
    })
}

// TODO read PVP option out of configuration
fn accept_login_arbiter(connection: Entity, region: Region) -> SingleEvent {
    Arc::new(Event::ResponseLoginArbiter {
        connection: Some(connection),
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
fn reject_login_arbiter(connection: Entity, region: Region) -> SingleEvent {
    Arc::new(Event::ResponseLoginArbiter {
        connection: Some(connection),
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

    use std::sync::Arc;

    use crate::ecs::component::{BatchEvent, SingleEvent};
    use crate::ecs::event::{self, Event};
    use crate::ecs::tag::EventKind;
    use crate::model::Region;
    use crate::protocol::packet::CCheckVersion;

    use legion::query::Read;
    use legion::systems::schedule::Schedule;
    use tokio::sync::mpsc::channel;

    fn setup() -> (World, Schedule, Resources) {
        let world = World::new();
        let schedule = Schedule::builder().add_system(init(world.id().index())).build();

        let mut resources = Resources::default();
        let map = HashMap::new();
        resources.insert(ConnectionMapping { map });

        (world, schedule, resources)
    }

    fn setup_with_connection() -> (World, Schedule, Entity, Resources) {
        let mut world = World::new();
        let schedule = Schedule::builder().add_system(init(world.id().index())).build();

        // FIXME There currently isn't a good insert method for one entity.
        let entities = world.insert(
            (),
            (0..1).map(|_| {
                (Connection {
                    verified: false,
                    version_checked: false,
                    region: None,
                    last_pong: Instant::now(),
                },)
            }),
        );
        let connection = entities[0];

        let mut resources = Resources::default();
        let map = HashMap::new();
        resources.insert(ConnectionMapping { map });

        (world, schedule, connection, resources)
    }

    #[test]
    fn test_connection_registration() {
        let (mut world, mut schedule, mut resources) = setup();
        let (tx_channel, _rx_channel) = channel(10);

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..5).map(|_| {
                (Arc::new(Event::RequestRegisterConnection {
                    connection: None,
                    response_channel: tx_channel.clone(),
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<SingleEvent>>::query();
        let count = query
            .iter(&world)
            .filter(|event| match ***event {
                Event::ResponseRegisterConnection { .. } => true,
                _ => false,
            })
            .count();

        assert_eq!(5, count);
    }

    #[test]
    fn test_check_version_valid() {
        let (mut world, mut schedule, connection, mut resources) = setup_with_connection();

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestCheckVersion {
                    connection: Some(connection),
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
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<Connection>>::query();
        let valid_component_count = query.iter(&world).filter(|component| component.version_checked).count();

        assert_eq!(1, valid_component_count);
    }

    #[test]
    fn test_check_version_invalid() {
        let (mut world, mut schedule, connection, mut resources) = setup_with_connection();

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestCheckVersion {
                    connection: Some(connection),
                    packet: CCheckVersion {
                        version: vec![CCheckVersionEntry {
                            index: 0,
                            value: 366_222,
                        }],
                    },
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<SingleEvent>>::query();
        let count = query
            .iter(&world)
            .filter(|event| match &***event {
                Event::ResponseCheckVersion { packet, .. } => !packet.ok,
                _ => false,
            })
            .count();

        assert_eq!(1, count);

        let query = <Read<Connection>>::query();
        let valid_component_count = query
            .iter(&world)
            .filter(|component| !component.version_checked)
            .count();

        assert_eq!(1, valid_component_count);
    }

    #[test]
    fn test_login_arbiter_valid() {
        let (mut world, mut schedule, connection, mut resources) = setup_with_connection();

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestLoginArbiter {
                    connection: Some(connection),
                    packet: CLoginArbiter {
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![
                            79, 83, 99, 71, 75, 116, 109, 114, 51, 115, 110, 103, 98, 52, 49, 56, 114, 70, 110, 72, 69,
                            68, 87, 77, 84, 114, 89, 83, 98, 72, 97, 50, 56, 48, 106, 118, 101, 90, 116, 67, 101, 71,
                            55, 84, 55, 112, 88, 118, 55, 72,
                        ],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<Connection>>::query();
        let valid_component_count = query.iter(&world).filter(|component| component.verified).count();

        assert_eq!(1, valid_component_count);
    }

    #[test]
    fn test_login_arbiter_invalid() {
        let (mut world, mut schedule, connection, mut resources) = setup_with_connection();

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestLoginArbiter {
                    connection: Some(connection),
                    packet: CLoginArbiter {
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<SingleEvent>>::query();
        let count = query
            .iter(&world)
            .filter(|event| match &***event {
                Event::ResponseLoginArbiter { packet, .. } => !packet.success,
                _ => false,
            })
            .count();

        assert_eq!(1, count);

        let query = <Read<Connection>>::query();
        let valid_component_count = query
            .iter(&world)
            .filter(|component| !component.version_checked)
            .count();

        assert_eq!(1, valid_component_count);
    }

    #[test]
    fn test_login_sequence() {
        let (mut world, mut schedule, mut resources) = setup();
        let (tx_channel, _rx_channel) = channel(10);

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestRegisterConnection {
                    connection: None,
                    response_channel: tx_channel.clone(),
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<SingleEvent>>::query();
        let mut con: Option<Entity> = None;
        for e in query.iter(&world) {
            match **e {
                Event::ResponseRegisterConnection { connection } => con = connection,
                _ => con = None,
            }
        }
        assert_ne!(None, con);

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestCheckVersion {
                    connection: con,
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
                }),)
            }),
        );

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestLoginArbiter {
                    connection: con,
                    packet: CLoginArbiter {
                        master_account_name: "royalBush5915".to_string(),
                        ticket: vec![],
                        unk1: 0,
                        unk2: 0,
                        region: Region::Europe,
                        patch_version: 9002,
                    },
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<BatchEvent>>::query();

        let count = query.iter(&world).count();
        assert_eq!(1, count);

        for batch in query.iter(&world) {
            assert_eq!(5, batch.len());

            if let Event::ResponseCheckVersion { connection, packet } = &*batch[0] {
                assert_eq!(con, *connection);
                assert_eq!(true, packet.ok);
            } else {
                panic!("received packets in from order");
            }
            if let Event::ResponseLoadingScreenControlInfo { connection, packet } = &*batch[1] {
                assert_eq!(con, *connection);
                assert_eq!(false, packet.custom_screen_enabled);
            } else {
                panic!("received packets in from order");
            }
            if let Event::ResponseRemainPlayTime { connection, packet } = &*batch[2] {
                assert_eq!(con, *connection);
                assert_eq!(6, packet.account_type);
            } else {
                panic!("received packets in from order");
            }
            if let Event::ResponseLoginArbiter { connection, packet } = &*batch[3] {
                assert_eq!(con, *connection);
                assert_eq!(true, packet.success);
                assert_eq!(65538, packet.status);
            } else {
                panic!("received packets in from order");
            }
            if let Event::ResponseLoginAccountInfo { connection, packet } = &*batch[4] {
                assert_eq!(con, *connection);
                assert_ne!(true, packet.server_name.trim().is_empty());
            } else {
                panic!("received packets in from order");
            }
        }
    }
}
