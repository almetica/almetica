use crate::ecs::event::EcsEvent;
use crate::ecs::resource::{EventRxChannel, WorldId};
use async_std::sync::TryRecvError;
use shipyard::*;
use tracing::{debug, info_span, trace};

/// Event receiver dispatches the events from the request channel into the ECS.
pub fn event_receiver_system(
    mut incoming_events: ViewMut<EcsEvent>,
    mut entities: EntitiesViewMut,
    event_channel: UniqueView<EventRxChannel>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    loop {
        match event_channel.channel.try_recv() {
            Ok(event) => {
                debug!("Created incoming event {}", event);
                trace!("Event data: {:?}", event);
                entities.add_entity(&mut incoming_events, event);
            }
            Err(TryRecvError::Empty) => {
                break;
            }
            Err(TryRecvError::Disconnected) => panic!("Event channel was disconnected"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::event::Event;
    use crate::ecs::resource::EventRxChannel;
    use crate::protocol::packet::CCheckVersion;
    use crate::Result;
    use async_std::sync::channel;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world
    }

    #[test]
    fn test_event_receiving() -> Result<()> {
        let world = setup();

        let (tx_channel, rx_channel) = channel(10);

        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        let entity = world.borrow::<EntitiesViewMut>().add_entity((), ());

        tx_channel.try_send(Box::new(Event::RequestCheckVersion {
            connection_id: entity,
            packet: CCheckVersion { version: vec![] },
        }))?;
        tx_channel.try_send(Box::new(Event::RequestCheckVersion {
            connection_id: entity,
            packet: CCheckVersion { version: vec![] },
        }))?;

        world.run(event_receiver_system);

        let count = world.borrow::<View<EcsEvent>>().iter().count();
        assert_eq!(count, 2);

        Ok(())
    }
}
