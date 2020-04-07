/// Event sender sends all outgoing events to the connection / local worlds.
use std::collections::HashMap;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error, info_span, warn};

use crate::ecs::component::{BatchEvent, SingleEvent};
use crate::ecs::event::{Event, EventKind};
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::tag;

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
                        warn!("Couldn't send event for connection because channel is closed");
                        connection_mapping.remove(&connection);
                        return;
                    }
                }
            }
            if let Event::ResponseDropConnection { connection } = **event {
                connection_mapping.remove(&connection.unwrap());
            }
        } else {
            debug!("Couldn't find a channel mapping for the connection");
        }
    } else {
        error!("Event didn't had an connection attached");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use legion::systems::schedule::Schedule;
    use tokio::sync::mpsc::{channel, Receiver};

    use crate::ecs::component::Connection;
    use crate::ecs::event::{self, Event};
    use crate::ecs::tag::EventKind;

    use super::*;

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
                last_pong: Instant::now(),
                waiting_for_pong: false,
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
    fn test_send_single_event() {
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
        while let Ok(event) = channel.try_recv() {
            match *event {
                Event::ResponseRegisterConnection { .. } => count += 1,
                _ => panic!("Couldn't find register connection event"),
            }
        }
        assert_eq!(10, count);
    }

    #[test]
    fn test_send_batch_event() {
        let (mut world, mut schedule, connection, mut resources, mut channel) = setup();

        world.insert(
            (EventKind(event::EventKind::Response),),
            (0..4).map(|_| {
                (vec![
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                    Arc::new(Event::ResponseRegisterConnection {
                        connection: Some(connection),
                    }),
                ],)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let mut count: u64 = 0;
        while let Ok(event) = channel.try_recv() {
            match *event {
                Event::ResponseRegisterConnection { .. } => count += 1,
                _ => panic!("Couldn't find register connection event"),
            }
        }
        assert_eq!(16, count);
    }

    #[test]
    fn test_drop_connection_event() {
        let (mut world, mut schedule, connection, mut resources, mut channel) = setup();

        world.insert(
            (EventKind(event::EventKind::Response),),
            (0..1).map(|_| {
                (Arc::new(Event::ResponseDropConnection {
                    connection: Some(connection),
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        // Connection was dropped
        let map = resources.get::<ConnectionMapping>().unwrap();
        assert_eq!(0, map.map.len());

        // ResponseDropConnection event was send
        let mut count: u64 = 0;
        while let Ok(event) = channel.try_recv() {
            match *event {
                Event::ResponseDropConnection { .. } => count += 1,
                _ => panic!("Couldn't find drop connection event"),
            }
        }
        assert_eq!(1, count);
    }
}
