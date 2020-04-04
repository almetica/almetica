/// Event sender sends all outgoing events to the connection / local worlds.
use std::collections::HashMap;

use crate::ecs::component::{SingleEvent, BatchEvent};
use crate::ecs::event::EventKind;
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::tag;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, error, info_span};

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventSender")
        .write_resource::<ConnectionMapping>()
        .with_query(<Read<SingleEvent>>::query().filter(tag_value(&tag::EventKind(EventKind::Response))))
        .with_query(<Read<BatchEvent>>::query().filter(tag_value(&tag::EventKind(EventKind::Response))))
        .build(move |_command_buffer, world, connection_mapping, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            // SingleEvent
            for event in queries.0.iter_mut(&mut *world) {
                let connection_map = &mut connection_mapping.map;
                send_event(&*event, connection_map);
            }

            // Batch Event
            for events in queries.1.iter_mut(&mut *world) {
                for event in events.iter() {
                    let connection_map = &mut connection_mapping.map;
                    send_event(event, connection_map);
                }
            }
        })
}

fn send_event(event: &SingleEvent, connection_mapping: &mut HashMap<Entity, Sender<SingleEvent>>) {
    if let Some(connection) = event.connection() {
        let span = info_span!("connection", %connection);
        let _enter = span.enter();

        if let Some(channel) = connection_mapping.get_mut(&connection) {
            debug!("Sending event {}", *event);
            if let Err(err) = channel.try_send(event.clone()) {
                match err {
                    TrySendError::Full(..) => {
                        error!("Dropping event for connection because channel is full");
                    }
                    TrySendError::Closed(..) => {
                        error!("Couldn't send event for connection because channel is closed");
                        connection_mapping.remove(&connection);
                    }
                }
            }
        } else {
            error!("Couldn't find a channel mapping for the connection");
        }
    } else {
        error!("Event didn't had an connection attached");
    }
}
