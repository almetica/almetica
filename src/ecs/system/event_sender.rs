use std::collections::HashMap;

use async_std::sync::Sender;
use async_std::task;
use shipyard::*;
use tracing::{debug, error, info_span};

use crate::ecs::component::*;
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::{ConnectionMapping, WorldId};

/// Event sender sends all outgoing events to the connection / local worlds.
pub fn event_sender_system(
    outgoing_events: View<OutgoingEvent>,
    mut connection_mapping: UniqueViewMut<ConnectionMapping>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    (&outgoing_events).iter().for_each(|event| {
        send_event_to_connection(&event, &mut connection_mapping.0);
    });
}

fn send_event_to_connection(
    event: &OutgoingEvent,
    connection_mapping: &mut HashMap<EntityId, Sender<EcsEvent>>,
) {
    if let Some(connection_id) = event.0.connection_id() {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        if let Some(channel) = connection_mapping.get_mut(&connection_id) {
            debug!("Sending event {}", *event.0);

            if !channel.is_full() {
                task::block_on(async {
                    channel.send(event.0.clone()).await;
                });
            } else {
                error!("Dropping event for connection because channel is full");
            }
            if let Event::ResponseDropConnection { connection_id } = *event.0 {
                connection_mapping.remove(&connection_id.unwrap());
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

    use async_std::sync::{channel, Receiver};
    use async_std::task;
    use shipyard::*;

    use crate::ecs::component::Connection;
    use crate::ecs::event::Event;

    use super::*;

    fn setup() -> (World, EntityId, Receiver<Arc<Event>>) {
        let world = World::new();
        world.add_unique(WorldId(0));

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        let (tx_channel, rx_channel) = channel(128);
        let mut map = HashMap::new();
        map.insert(connection_id, tx_channel);

        world.add_unique(ConnectionMapping(map));

        (world, connection_id, rx_channel)
    }

    #[test]
    fn test_send_single_event() {
        let (world, connection_id, channel) = setup();

        world.run(
            |mut entities: EntitiesViewMut, mut events: ViewMut<OutgoingEvent>| {
                for _i in 0..10 {
                    entities.add_entity(
                        &mut events,
                        OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
                            connection_id: Some(connection_id),
                        })),
                    );
                }
            },
        );

        world.run(event_sender_system);

        let mut count: u64 = 0;
        task::block_on(async {
            while !channel.is_empty() {
                if let Some(event) = channel.recv().await {
                    match *event {
                        Event::ResponseRegisterConnection { .. } => count += 1,
                        _ => panic!("Couldn't find register connection event"),
                    }
                }
            }
        });
        assert_eq!(count, 10);
    }

    #[test]
    fn test_drop_connection_event() {
        let (world, connection_id, channel) = setup();

        world.run(
            |mut entities: EntitiesViewMut, mut events: ViewMut<OutgoingEvent>| {
                for _i in 0..1 {
                    entities.add_entity(
                        &mut events,
                        OutgoingEvent(Arc::new(Event::ResponseDropConnection {
                            connection_id: Some(connection_id),
                        })),
                    );
                }
            },
        );

        world.run(event_sender_system);

        // Connection was dropped
        assert_eq!(world.borrow::<UniqueView<ConnectionMapping>>().0.len(), 0);

        // ResponseDropConnection event was send
        let mut count: u64 = 0;
        task::block_on(async {
            while !channel.is_empty() {
                if let Some(event) = channel.recv().await {
                    match *event {
                        Event::ResponseDropConnection { .. } => count += 1,
                        _ => panic!("Couldn't find drop connection event"),
                    }
                }
            }
        });
        assert_eq!(count, 1);
    }
}
