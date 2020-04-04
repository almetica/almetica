/// Event receiver dispatches the events from the Request channel into the ECS.
use crate::ecs::event::EventKind;
use crate::ecs::resource::EventRxChannel;
use crate::ecs::tag;

use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::{debug, error, info_span};

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventReceiver")
        .write_resource::<EventRxChannel>()
        .build(move |command_buffer, _world, event_channel, _queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            loop {
                match event_channel.channel.try_recv() {
                    Ok(event) => {
                        debug!("Received event {}", event);
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
                                error!("Request channel was closed");
                            }
                        }
                    }
                }
            }
        })
}

// TODO Event emitting test
