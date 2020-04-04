/// Event sender sends all outgoing events to the connection / local worlds.
use std::collections::HashMap;

use crate::ecs::component::{BatchEvent, SingleEvent};
use crate::ecs::event::EventKind;
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::tag;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::ecs::component::Connection;
    use crate::ecs::event::{self, Event};
    use crate::ecs::tag::EventKind;
    use legion::systems::schedule::Schedule;
    use tokio::sync::mpsc::{channel, Receiver};

    fn setup() -> (World, Schedule, Entity, Resources, Receiver<Arc<Event>>) {
        let mut world = World::new();
        let schedule = Schedule::builder().add_system(init(world.id().index())).build();

        // FIXME There currently isn't a good insert method for one entity.
        let entities = world.insert(
            (),
            vec![(Connection {
                verified: false,
                version_checked: false,
                region: None,
            },)],
        );
        let connection = entities[0];

        let (tx_channel, rx_channel) = channel(128);
        let mut map = HashMap::new();
        map.insert(connection, tx_channel);

        let mut resources = Resources::default();
        resources.insert(ConnectionMapping { map });

        (world, schedule, connection, resources, rx_channel)
    }

    #[test]
    fn test_event_sender_single_event() {
        let (mut world, mut schedule, connection, mut resources, mut channel) = setup();

        world.insert(
            (EventKind(event::EventKind::Response),),
            (0..10).map(|_| {
                (Arc::new(Event::ResponseRegisterConnection {
                    connection: Some(connection),
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let mut count: u64 = 0;
        loop {
            match channel.try_recv() {
                Ok(_) => count += 1,
                Err(_) => break,
            }
        }
        assert_eq!(10, count);
    }

    #[test]
    fn test_event_sender_batch_event() {
        let (mut world, mut schedule, connection, mut resources, mut channel) = setup();

        world.insert(
            (EventKind(event::EventKind::Response),),
            (0..4).map(|_| {
                (vec![
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseDropConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseDropConnection {
                        connection: Some(connection),
                    }),
                ],)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let mut count: u64 = 0;
        loop {
            match channel.try_recv() {
                Ok(_) => count += 1,
                Err(_) => break,
            }
        }
        assert_eq!(16, count);
    }
}
