use std::sync::Arc;

use crate::ecs::event::{Event, EventKind};
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::tag;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, error, info_span};

/// Event sender sends all outgoing events to the connection / local worlds.
pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventSender")
        .write_resource::<ConnectionMapping>()
        .with_query(<Read<Arc<Event>>>::query().filter(tag_value(&tag::EventKind(EventKind::Response))))
        .build(move |_command_buffer, world, connection_mapping, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            for event in queries.iter_mut(&mut *world) {
                // TODO handle system events between the ECS
                if let Some(uid) = event.uid() {
                    let span = info_span!("connection", uid);
                    let _enter = span.enter();

                    let connection_map = &mut connection_mapping.map;
                    if let Some(channel) = connection_map.get_mut(&uid) {
                        debug!("Sending event {}", *event);
                        let e = &*event;
                        if let Err(err) = channel.try_send(e.clone()) {
                            match err {
                                TrySendError::Full(..) => {
                                    error!("Dropping event for connection because channel is full");
                                }
                                TrySendError::Closed(..) => {
                                    error!("Couldn't send event for connection because channel is closed");
                                    connection_map.remove(&uid);
                                }
                            }
                        }
                    } else {
                        error!("Couldn't find channel mapping for connection");
                    }
                } else {
                    error!("Event didn't had an uid attached");
                }
            }
        })
}
