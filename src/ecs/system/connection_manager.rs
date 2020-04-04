/// Connection handler handles the connection components.
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;

use crate::ecs::component::{Connection, SingleEvent, BatchEvent};
use crate::ecs::event::Event;
use crate::ecs::event::EventKind;
use crate::ecs::resource::ConnectionMapping;
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
        .write_component::<SingleEvent>()
        .write_component::<BatchEvent>()
        .write_component::<Connection>()
        .build(move |mut command_buffer, mut world, connection_mapping, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            for event in queries.iter_mut(&mut *world) {
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
                    _ => { /* Ignore all other events */ }
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

fn send_event(event: SingleEvent, command_buffer: &mut CommandBuffer) {
    debug!("Created {} event", event);
    trace!("Event data: {}", event);
    command_buffer
        .start_entity()
        .with_tag((tag::EventKind(EventKind::Response),))
        .with_component((event,))
        .build();
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

// TODO Registration test
