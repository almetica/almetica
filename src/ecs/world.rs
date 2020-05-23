/// Module that handles the world generation and handling
use crate::config::Configuration;
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::resource::*;
use crate::ecs::system::{common, global, local};
use async_std::sync::{channel, Sender};
use shipyard::*;
use sqlx::PgPool;
use std::time::Duration;
use std::{thread, time};
use tracing::{error, info, info_span};

const GLOBAL_WORLD_TICK_RATE: u64 = 10;
const LOCAL_WORLD_TICK_RATE: u64 = 30;

/// The global world handles all general messages and the persistence layer.
pub struct GlobalWorld {
    pub channel: Sender<EcsMessage>,
    pub world: World,
}

impl GlobalWorld {
    /// Creates a new GlobalWorld.
    pub fn new(config: &Configuration, pool: &PgPool) -> Self {
        let world = World::new();
        info!("Creating global world");

        // Create channels to send data to and from the global world.
        // At most 16384 messages can be queued between server ticks
        let (tx_channel, rx_channel) = channel(16384);
        world.add_unique(InputChannel {
            channel: rx_channel,
        });
        world.add_unique(GlobalMessageChannel {
            channel: tx_channel.clone(),
        });
        world.add_unique(ShutdownSignal {
            status: ShutdownSignalStatus::Operational,
        });
        world.add_unique(config.clone());
        world.add_unique(pool.clone());

        let vec: Vec<EntityId> = Vec::with_capacity(4096);
        world.add_unique(DeletionList(vec));

        Self {
            channel: tx_channel,
            world,
        }
    }

    /// Starts the main loop of the global world.
    pub fn run(&mut self) {
        let span = info_span!("world", world_id = "global");
        let _enter = span.enter();

        let world = &mut self.world;

        // Build the workload
        const GLOBAL_WORLD_TICK: &str = "GLOBAL_WORLD_TICK";
        world
            .add_workload(GLOBAL_WORLD_TICK)
            .with_system(system!(common::message_receiver_system))
            .with_system(system!(global::connection_manager_system))
            .with_system(system!(global::settings_manager_system))
            .with_system(system!(global::user_manager_system))
            .with_system(system!(global::user_spawner_system))
            .with_system(system!(global::local_world_manager_system))
            .with_system(system!(common::cleaner_system))
            .build();

        let min_tick_duration = time::Duration::from_millis(1000 / GLOBAL_WORLD_TICK_RATE);
        loop {
            let shutdown_signal = world.borrow::<UniqueView<ShutdownSignal>>();
            if shutdown_signal.status == ShutdownSignalStatus::Shutdown {
                info!("Shutting down the global world");
                break;
            }
            drop(shutdown_signal);

            run_workload_tick(&world, GLOBAL_WORLD_TICK, min_tick_duration);
        }
    }

    /// Get the Input Message Channel of the global world.
    pub fn get_global_input_message_channel(&self) -> Sender<EcsMessage> {
        self.channel.clone()
    }
}

/// LocalWorld handles all combat and instance related messages.
pub struct LocalWorld {
    pub id: EntityId,
    pub channel: Sender<EcsMessage>,
    pub world: World,
}

impl LocalWorld {
    /// Creates a new LocalWorld.
    pub fn new(
        config: &Configuration,
        pool: &PgPool,
        world_id: EntityId,
        global_world_channel: Sender<EcsMessage>,
    ) -> Self {
        let world = World::new();
        info!("Creating local world {:?}", world_id);

        // Create channels to send data to and from the local world.
        // At most 8192 messages can be queued between server ticks.
        let (tx_channel, rx_channel) = channel(8192);
        world.add_unique(InputChannel {
            channel: rx_channel,
        });
        world.add_unique(GlobalMessageChannel {
            channel: global_world_channel,
        });
        world.add_unique(ShutdownSignal {
            status: ShutdownSignalStatus::Operational,
        });
        world.add_unique(config.clone());
        world.add_unique(pool.clone());

        let vec: Vec<EntityId> = Vec::with_capacity(4096);
        world.add_unique(DeletionList(vec));

        Self {
            id: world_id,
            channel: tx_channel,
            world,
        }
    }

    /// Starts the main loop of the local world.
    pub fn run(&mut self) {
        let span = info_span!("world", world_id = ?self.id);
        let _enter = span.enter();

        let world = &mut self.world;

        // Build the workload
        const LOCAL_WORLD_TICK: &str = "LOCAL_WORLD_TICK";
        world
            .add_workload(LOCAL_WORLD_TICK)
            .with_system(system!(common::message_receiver_system))
            .with_system(system!(local::user_gateway_system))
            .with_system(system!(common::cleaner_system))
            .with_system(system!(common::shutdown_system))
            .build();

        info!("Loading data for local world {:?}", self.id);
        // TODO Load all additional data that the local world needs
        info!("Finished loading data for local world {:?}", self.id);

        // Inform the global world that we finished loading and can accept messages
        let global_message_channel = world.borrow::<UniqueView<GlobalMessageChannel>>();
        match global_message_channel
            .channel
            .try_send(Box::new(Message::LocalWorldLoaded {
                successful: true,
                global_world_id: self.id,
            })) {
            Ok(..) => {}
            Err(e) => {
                error!(
                    "Can't send Message::LocalWorldLoaded to global world: {:?}",
                    e
                );
                return;
            }
        }
        drop(global_message_channel);

        let min_tick_duration = time::Duration::from_millis(1000 / LOCAL_WORLD_TICK_RATE);
        loop {
            let shutdown_signal = world.borrow::<UniqueView<ShutdownSignal>>();
            if shutdown_signal.status == ShutdownSignalStatus::Shutdown {
                info!("Shutting down local world {:?}", self.id);
                break;
            }
            drop(shutdown_signal);

            run_workload_tick(&world, LOCAL_WORLD_TICK, min_tick_duration);
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
