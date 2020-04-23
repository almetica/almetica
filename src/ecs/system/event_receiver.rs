use shipyard::*;
use tokio::sync::mpsc::error::TryRecvError;
use tracing::{debug, error, info_span, trace};

use crate::ecs::component::IncomingEvent;
use crate::ecs::resource::{EventRxChannel, WorldId};

/// Event receiver dispatches the events from the request channel into the ECS.
pub fn event_receiver_system(
    mut incoming_events: ViewMut<IncomingEvent>,
    mut entities: EntitiesViewMut,
    mut event_channel: UniqueViewMut<EventRxChannel>,
    world_id: UniqueView<WorldId>,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shipyard::*;
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

        world.run(event_receiver_system);

        let count = world.borrow::<View<IncomingEvent>>().iter().count();
        assert_eq!(count, 2);
    }
}
