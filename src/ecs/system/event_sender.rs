/// Event sender sends all outgoing events to the connection / local worlds.
use std::collections::HashMap;

use shipyard::prelude::*;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error, info_span, warn};

use crate::ecs::component::*;
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::{ConnectionMapping, WorldId};

#[system(EventSender)]
pub fn run(
    outgoing_events: &OutgoingEvent,
    mut connection_mapping: Unique<&mut ConnectionMapping>,
    world_id: Unique<&WorldId>,
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

            if let Err(err) = channel.try_send(event.0.clone()) {
                match err {
                    TrySendError::Full(..) => {
                        error!("Dropping event for connection because channel is full");
                    }
                    TrySendError::Closed(..) => {
                        warn!("Couldn't send event for connection because channel is closed");
                        connection_mapping.remove(&connection_id);
                        return;
                    }
                }
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

    use shipyard::prelude::*;
    use tokio::sync::mpsc::{channel, Receiver};

    use crate::ecs::component::Connection;
    use crate::ecs::event::Event;

    use super::*;

    fn setup() -> (World, EntityId, Receiver<Arc<Event>>) {
        let world = World::new();
        world.add_unique(WorldId(0));

        let connection_id = world.run::<(EntitiesMut, &mut Connection), EntityId, _>(
            |(mut entities, mut connections)| {
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
        let (world, connection_id, mut channel) = setup();

        world.run::<(EntitiesMut, &mut OutgoingEvent), _, _>(|(mut entities, mut events)| {
            for _i in 0..10 {
                entities.add_entity(
                    &mut events,
                    OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
                        connection_id: Some(connection_id),
                    })),
                );
            }
        });

        world.run_system::<EventSender>();

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
    fn test_drop_connection_event() {
        let (world, connection_id, mut channel) = setup();

        world.run::<(EntitiesMut, &mut OutgoingEvent), _, _>(|(mut entities, mut events)| {
            for _i in 0..1 {
                entities.add_entity(
                    &mut events,
                    OutgoingEvent(Arc::new(Event::ResponseDropConnection {
                        connection_id: Some(connection_id),
                    })),
                );
            }
        });

        world.run_system::<EventSender>();

        // Connection was dropped
        assert_eq!(0, world.borrow::<Unique<&ConnectionMapping>>().0.len());

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
