use std::collections::HashMap;
/// Module that handles the world generation and handling
use std::{thread, time};

use crate::ecs::component::SingleEvent;
use crate::ecs::resource::*;
use crate::ecs::system::*;

use legion::prelude::*;
use tokio::sync::mpsc::{channel, Sender};
use tracing::debug;

/// Holds the ECS for the global world and all instanced worlds.
pub struct Multiverse {
    _universe: Universe,
    pub(crate) global_world_handle: WorldHandle,
    pub(crate) _instanced_world_handles: HashMap<String, World>,
    pub(crate) resources: Resources,
}

impl Multiverse {
    /// Creates a new Multiverse.
    pub fn new() -> Multiverse {
        Default::default()
    }

    /// Starts the main loop of the global world.
    pub fn run(&mut self) {
        let world_id = self.global_world_handle.world.id().index();
        let mut schedule = Schedule::builder()
            .add_system(event_receiver::init(world_id))
            .flush()
            .add_system(connection_manager::init(world_id))
            .flush()
            // General system start here
            .add_system(settings_manager::init(world_id))
            .add_system(user_manager::init(world_id))
            .flush()
            .add_system(event_sender::init(world_id))
            .flush()
            .add_system(event_cleaner::init(world_id))
            .build();

        // Global tick rate is at best 50ms (20 Hz)
        let min_duration = time::Duration::from_millis(50);
        loop {
            let start = time::Instant::now();
            schedule.execute(&mut self.global_world_handle.world, &mut self.resources);

            let elapsed = start.elapsed();
            if elapsed < min_duration {
                thread::sleep(min_duration - elapsed);
            }
        }
    }

    /// Get the Input Event Channel of the global world
    pub fn get_global_input_event_channel(&self) -> Sender<SingleEvent> {
        self.global_world_handle.tx_channel.clone()
    }
}

impl Default for Multiverse {
    fn default() -> Self {
        let universe = Universe::new();
        let world = universe.create_world();

        debug!("Global world with ID {} created", world.id().index());

        // Create channels to send data to and from the global world.
        // At most 1024 events can be queued between server ticks
        let (tx_channel, rx_channel) = channel(1024);
        let mut resources = Resources::default();
        resources.insert(EventRxChannel { channel: rx_channel });
        resources.insert(ConnectionMapping {
            map: HashMap::with_capacity(128),
        });

        Multiverse {
            _universe: universe,
            global_world_handle: WorldHandle { tx_channel, world },
            _instanced_world_handles: HashMap::new(),
            resources,
        }
    }
}

/// Handle for a world.
/// Connections can register their connection by using the `Èvent::RegisterConnection` event.
pub struct WorldHandle {
    pub tx_channel: Sender<SingleEvent>,
    pub world: World,
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::ecs::event::Event;
    use crate::Result;
    use tokio::sync::mpsc::channel;

    #[test]
    fn test_multiverse_creation() -> Result<()> {
        let mut m = Multiverse::new();
        let (tx, _) = channel(128);

        match m
            .global_world_handle
            .tx_channel
            .try_send(Arc::new(Event::RequestRegisterConnection {
                connection: None,
                response_channel: tx,
            })) {
            Ok(()) => Ok(()),
            Err(e) => panic!(e),
        }
    }
}
