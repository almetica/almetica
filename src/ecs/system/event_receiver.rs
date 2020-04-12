/// Event receiver dispatches the events from the Request channel into the ECS.
use shipyard::prelude::*;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::{debug, error, info_span, trace};

use crate::ecs::component::IncomingEvent;
use crate::ecs::resource::{EventRxChannel, WorldId};

pub struct EventReceiver;

impl<'sys> System<'sys> for EventReceiver {
    type Data = (
        &'sys mut IncomingEvent,
        EntitiesMut,
        Unique<&'sys mut EventRxChannel>,
        Unique<&'sys WorldId>,
    );

    fn run(
        (mut incoming_events, mut entities, mut event_channel, world_id): <Self::Data as SystemData<
            'sys,
        >>::View,
    ) {
        let span = info_span!("world", world_id = world_id.0);
        let _enter = span.enter();

        loop {
            match event_channel.channel.try_recv() {
                Ok(event) => {
                    debug!("Created incoming event {}", event);
                    trace!("Event data: {:?}", event);
                    entities.add_entity(&mut incoming_events, IncomingEvent(event));
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
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shipyard::prelude::*;
    use tokio::sync::mpsc::channel;

    use crate::ecs::event::Event;
    use crate::ecs::resource::EventRxChannel;
    use crate::protocol::packet::CCheckVersion;

    use super::*;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world
    }

    #[test]
    fn test_event_receiving() {
        let world = setup();

        let (mut tx_channel, rx_channel) = channel(10);

        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        tx_channel
            .try_send(Arc::new(Event::RequestCheckVersion {
                connection_id: None,
                packet: CCheckVersion { version: vec![] },
            }))
            .unwrap();
        tx_channel
            .try_send(Arc::new(Event::RequestCheckVersion {
                connection_id: None,
                packet: CCheckVersion { version: vec![] },
            }))
            .unwrap();

        world.run_system::<EventReceiver>();

        let count = world.borrow::<&IncomingEvent>().iter().count();
        assert_eq!(2, count);
    }
}
