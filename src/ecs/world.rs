/// Module that handles the world generation and handling
use std::collections::HashMap;
use std::{thread, time};

use shipyard::*;
use tokio::sync::mpsc::{channel, Sender};
use tracing::debug;

use crate::config::Configuration;
use crate::ecs::event::EcsEvent;
use crate::ecs::resource::*;
use crate::ecs::system::*;
use crate::SyncDbPool;

/// Holds the ECS for the global world and all instanced worlds.
pub struct Multiverse {
    pub(crate) global_handle: WorldHandle,
    pub(crate) _instanced_handles: HashMap<String, World>,
}

impl Multiverse {
    /// Creates a new Multiverse.
    pub fn new() -> Multiverse {
        Default::default()
    }

    /// Starts the main loop of the global world.
    pub fn run(&mut self, pool: SyncDbPool, config: Configuration) {
        let world = &mut self.global_handle.world;

        // Copy configuration and db pool into the global resources so that systems can access them.
        world.add_unique(config);
        world.add_unique(pool);

        // Global tick rate is at best 50ms (20 Hz)
        let min_duration = time::Duration::from_millis(50);
        loop {
            let start = time::Instant::now();

            world
                .add_workload("global world tick")
                .with_system(system!(event_receiver_system))
                .with_system(system!(connection_manager_system))
                .with_system(system!(settings_manager_system))
                .with_system(system!(user_manager_system))
                .with_system(system!(event_sender_system))
                .with_system(system!(cleaner_system))
                .build();

            let elapsed = start.elapsed();
            if elapsed < min_duration {
                thread::sleep(min_duration - elapsed);
            }
        }
    }

    /// Get the Input Event Channel of the global world
    pub fn get_global_input_event_channel(&self) -> Sender<EcsEvent> {
        self.global_handle.tx_channel.clone()
    }
}

impl Default for Multiverse {
    fn default() -> Self {
        let world = World::new();
        let id = 0;
        debug!("Global world created with ID {}", id);

        // Create channels to send data to and from the global world.
        // At most 1024 events can be queued between server ticks
        let (tx_channel, rx_channel) = channel(1024);

        world.add_unique(WorldId(id));
        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        let map: HashMap<EntityId, Sender<EcsEvent>> = HashMap::with_capacity(512);
        world.add_unique(ConnectionMapping(map));

        let vec: Vec<EntityId> = Vec::with_capacity(512);
        world.add_unique(DeletionList(vec));

        Multiverse {
            global_handle: WorldHandle {
                id,
                tx_channel,
                world,
            },
            _instanced_handles: HashMap::new(),
        }
    }
}

/// Handle for a world.
/// Connections can register their connection by using the `Event::RegisterConnection` event.
pub struct WorldHandle {
    pub id: u64,
    pub tx_channel: Sender<EcsEvent>,
    pub world: World,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc::channel;

    use crate::ecs::event::Event;
    use crate::Result;

    use super::*;

    #[test]
    fn test_multiverse_creation() -> Result<()> {
        let mut m = Multiverse::new();
        let (tx, _) = channel(128);

        match m
            .global_handle
            .tx_channel
            .try_send(Arc::new(Event::RequestRegisterConnection {
                connection_id: None,
                response_channel: tx,
            })) {
            Ok(()) => Ok(()),
            Err(e) => panic!(e),
        }
    }
}
