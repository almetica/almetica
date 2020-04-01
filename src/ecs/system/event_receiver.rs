use crate::ecs::event::EventKind;
use crate::ecs::resource::EventRxChannel;
use crate::ecs::tag;

use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use legion::world::WorldId;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::{debug, error};

/// Event receiver dispatches the events from the Request channel into the ECS.
pub fn init(world_id: WorldId) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventReceiver")
        .write_resource::<EventRxChannel>()
        .build(move |command_buffer, _world, event_channel, _queries| {
            loop {
                match event_channel.channel.try_recv() {
                    Ok(event) => {
                        debug!("Received event {} for {:?}", event, world_id);
                        command_buffer
                            .start_entity()
                            .with_tag((tag::EventKind(EventKind::Request),))
                            .with_component((event,))
                            .build();
                    }
                    Err(e) => {
                        match e {
                            TryRecvError::Empty => {
                                /* Nothing to do */
                                return;
                            }
                            TryRecvError::Closed => {
                                error!("Request channel was closed for {:?}", world_id);
                            }
                        }
                    }
                }
            }
        })
}

// TODO Event emitting test
