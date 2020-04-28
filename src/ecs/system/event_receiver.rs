use async_std::task;
use shipyard::*;
use tracing::{debug, info_span, trace};

use crate::ecs::component::IncomingEvent;
use crate::ecs::resource::{EventRxChannel, WorldId};

/// Event receiver dispatches the events from the request channel into the ECS.
pub fn event_receiver_system(
    mut incoming_events: ViewMut<IncomingEvent>,
    mut entities: EntitiesViewMut,
    event_channel: UniqueView<EventRxChannel>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    loop {
        if event_channel.channel.is_empty() {
            break;
        }

        task::block_on(async {
            if let Some(event) = event_channel.channel.recv().await {
                debug!("Created incoming event {}", event);
                trace!("Event data: {:?}", event);
                entities.add_entity(&mut incoming_events, IncomingEvent(event));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_std::sync::channel;
    use async_std::task;
    use shipyard::*;

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

        let (tx_channel, rx_channel) = channel(10);

        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        let entity = world.borrow::<EntitiesViewMut>().add_entity((), ());

        task::block_on(async {
            tx_channel
                .send(Arc::new(Event::RequestCheckVersion {
                    connection_id: entity,
                    packet: CCheckVersion { version: vec![] },
                }))
                .await;
            tx_channel
                .send(Arc::new(Event::RequestCheckVersion {
                    connection_id: entity,
                    packet: CCheckVersion { version: vec![] },
                }))
                .await;
        });

        world.run(event_receiver_system);

        let count = world.borrow::<View<IncomingEvent>>().iter().count();
        assert_eq!(count, 2);
    }
}
