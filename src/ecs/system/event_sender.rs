use std::sync::Arc;

use crate::ecs::event::{Event, EventKind};
use crate::ecs::tag;
use crate::ecs::resource::ConnectionMapping;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use legion::world::WorldId;
use log::{debug, error};
use tokio::sync::mpsc::error::TrySendError;

/// Event sender sends all outgoing events to the connection / local worlds.
pub fn init(id: WorldId) -> Box<dyn Schedulable> {
    let world_id = id;
    SystemBuilder::new("EventSender")
        .write_resource::<ConnectionMapping>()
        .with_query(<Read<Arc<Event>>>::query().filter(tag_value(&tag::EventKind(EventKind::Response))))
        .build(move |_command_buffer, world, connection_mapping, queries| {
            for event in queries.iter_mut(&mut *world) {
                // TODO handle system events between the ECS
                if let Some(uid) = event.uid() {
                    match connection_mapping.map.get_mut(&uid) {
                        Some(channel) => {
                            debug!("Sending event {} on {:?}", *event, world_id);
                            // TODO this is the reason why we use Arc and not Box.
                            // We would otherwhise copy the boxed value,
                            // since we can't move the value here.
                            let e = &*event;
                            if let Err(err) = channel.try_send(e.clone()) {
                                match err {
                                    TrySendError::Full(..) => {
                                        error!("Dropping event for connection with UID {} because channel is full on {:?}", uid, world_id);
                                    },
                                    TrySendError::Closed(..) => {
                                        error!("Couldn't send event for connection with UID {} because channel is closed on {:?}", uid, world_id);
                                        connection_mapping.map.remove(&uid);
                                    }
                                }
                            }
                        },
                        None => {
                            error!("Couldn't find channel mapping for connection with UID {} on {:?}", uid, world_id);
                        } 
                    }
                } else {
                    error!("Event didn't had an UID attached on {:?}", world_id);
                }
            }
        })
}
