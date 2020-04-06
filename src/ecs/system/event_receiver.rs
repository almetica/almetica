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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::ecs::component::SingleEvent;
    use crate::ecs::event::Event;
    use crate::ecs::resource::EventRxChannel;
    use crate::protocol::packet::CCheckVersion;

    use legion::prelude::*;
    use legion::query::Read;
    use legion::systems::schedule::Schedule;
    use tokio::sync::mpsc::channel;

    fn setup() -> (World, Schedule) {
        let world = World::new();
        let schedule = Schedule::builder().add_system(init(world.id().index())).build();
        (world, schedule)
    }

    #[test]
    fn test_event_receiver() {
        let (mut world, mut schedule) = setup();

        let (mut tx_channel, rx_channel) = channel(10);
        let mut resources = Resources::default();
        resources.insert(EventRxChannel { channel: rx_channel });

        tx_channel
            .try_send(Arc::new(Event::RequestCheckVersion {
                connection: None,
                packet: CCheckVersion { version: vec![] },
            }))
            .unwrap();
        tx_channel
            .try_send(Arc::new(Event::RequestCheckVersion {
                connection: None,
                packet: CCheckVersion { version: vec![] },
            }))
            .unwrap();

        schedule.execute(&mut world, &mut resources);

        let query = <(Read<SingleEvent>,)>::query();
        let count = query.iter(&world).count();
        assert_eq!(2, count);
    }
}
