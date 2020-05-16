/// Module that handles the world generation and handling
use crate::config::Configuration;
use crate::ecs::event::EcsEvent;
use crate::ecs::resource::*;
use crate::ecs::system::*;
use async_std::sync::{channel, Sender};
use shipyard::*;
use sqlx::PgPool;
use std::time::Duration;
use std::{thread, time};
use tracing::debug;

/// The global world handles all general events and the persistence layer.
pub struct GlobalWorld {
    pub id: u64,
    pub tx_channel: Sender<EcsEvent>,
    pub world: World,
}

impl GlobalWorld {
    /// Creates a new GlobalWorld.
    pub fn new() -> GlobalWorld {
        Default::default()
    }

    /// Starts the main loop of the global world.
    pub fn run(&mut self, pool: PgPool, config: Configuration) {
        let world = &mut self.world;

        // Copy configuration and db pool into the global resources so that systems can access them.
        world.add_unique(config);
        world.add_unique(pool);

        // Build the workload
        const GLOBAL_WORLD_TICK: &str = "GLOBAL_WORLD_TICK";
        world
            .add_workload(GLOBAL_WORLD_TICK)
            .with_system(system!(event_receiver_system))
            .with_system(system!(connection_manager_system))
            .with_system(system!(settings_manager_system))
            .with_system(system!(user_manager_system))
            .with_system(system!(cleaner_system))
            .build();

        // Global tick rate is at best 100ms (10 Hz)
        let min_duration = time::Duration::from_millis(100);
        loop {
            let start = time::Instant::now();

            world.run_workload(GLOBAL_WORLD_TICK);

            let elapsed = start.elapsed();
            if elapsed < min_duration {
                thread::sleep(min_duration - elapsed);
            }
        }
    }

    /// Get the Input Event Channel of the global world
    pub fn get_global_input_event_channel(&self) -> Sender<EcsEvent> {
        self.tx_channel.clone()
    }
}

impl Default for GlobalWorld {
    fn default() -> Self {
        let world = World::new();
        let id = 0;
        debug!("Global world created with ID {}", id);

        // Create channels to send data to and from the global world.
        // At most 16384 events can be queued between server ticks
        let (tx_channel, rx_channel) = channel(16384);

        world.add_unique(WorldId(id));
        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        let vec: Vec<EntityId> = Vec::with_capacity(4096);
        world.add_unique(DeletionList(vec));

        GlobalWorld {
            id,
            tx_channel,
            world,
        }
    }
}

/// LocalWorld handles all combat and instance related events.
pub struct LocalWorld {
    pub id: u64,
    pub tx_channel: Sender<EcsEvent>,
    pub world: World,
}

impl LocalWorld {
    /// Creates a new LocalWorld.
    pub fn new() -> LocalWorld {
        Default::default()
    }

    /// Starts the main loop of the local world.
    pub fn run(&mut self, pool: PgPool, config: Configuration) {
        let world = &mut self.world;

        // Copy configuration and db pool into the global resources so that systems can access them.
        world.add_unique(config);
        world.add_unique(pool);

        // Build the workload
        const LOCAL_WORLD_TICK: &str = "LOCAL_WORLD_TICK";
        world
            .add_workload(LOCAL_WORLD_TICK)
            .with_system(system!(event_receiver_system))
            .with_system(system!(cleaner_system))
            .build();

        // Global tick rate is at best 33ms (30 Hz)
        let min_tick_duration = time::Duration::from_millis(50);
        loop {
            run_workload_tick(&world, LOCAL_WORLD_TICK, min_tick_duration);
        }
    }

    /// Get the Input Event Channel of the global world
    pub fn get_global_input_event_channel(&self) -> Sender<EcsEvent> {
        self.tx_channel.clone()
    }
}

impl Default for LocalWorld {
    fn default() -> Self {
        let world = World::new();
        let id = 0;
        debug!("Local world created with ID {}", id);

        // Create channels to send data to and from the global world.
        // At most 8192 events can be queued between server ticks
        let (tx_channel, rx_channel) = channel(8192);

        world.add_unique(WorldId(id));
        world.add_unique(EventRxChannel {
            channel: rx_channel,
        });

        let vec: Vec<EntityId> = Vec::with_capacity(4096);
        world.add_unique(DeletionList(vec));

        LocalWorld {
            id,
            tx_channel,
            world,
        }
    }
}

#[inline]
fn run_workload_tick(world: &World, workload_name: &str, min_tick_duration: Duration) {
    let start = time::Instant::now();

    world.run_workload(workload_name);

    let elapsed = start.elapsed();
    if elapsed < min_tick_duration {
        thread::sleep(min_tick_duration - elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::event::Event;
    use crate::Result;
    use async_std::future::timeout;
    use async_std::sync::channel;
    use std::time::Duration;

    #[async_std::test]
    async fn test_global_world_creation() -> Result<()> {
        let m = GlobalWorld::new();
        let (tx, _) = channel(128);

        let future = m
            .tx_channel
            .send(Box::new(Event::RequestRegisterConnection {
                response_channel: tx,
            }));

        timeout(Duration::from_millis(100), future).await?;

        Ok(())
    }

    #[async_std::test]
    async fn test_local_world_creation() -> Result<()> {
        let m = LocalWorld::new();
        let (tx, _) = channel(128);

        let future = m
            .tx_channel
            .send(Box::new(Event::RequestRegisterConnection {
                response_channel: tx,
            }));

        timeout(Duration::from_millis(100), future).await?;

        Ok(())
    }
}
