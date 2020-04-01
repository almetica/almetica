/// Module that handles the world generation and handling
use std::collections::HashMap;
use std::{thread, time};

use super::event::Event;
use super::resource::EventRxChannel;
use super::system::*;

use legion::prelude::*;
use log::debug;
use tokio::sync::mpsc::{channel, Sender};

/// Holds the ECS for the global world and all instanced worlds.
pub struct Multiverse {
    _universe: Universe,
    global_world_handle: WorldHandle,
    _instanced_world_handles: HashMap<String, World>,
    resources: Resources,
}

impl Multiverse {
    /// Creates a new Multiverse.
    pub fn new() -> Multiverse {
        Default::default()
    }

    /// Starts the main loop of the global world.
    pub fn run(&mut self) {
        let mut schedule = Schedule::builder()
            .add_system(event_handler::init(&self.global_world_handle.world))
            .flush()
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
    pub fn get_global_input_event_channel(&self) -> Sender<Box<Event>> {
        self.global_world_handle.tx_channel.clone()
    }
}

impl Default for Multiverse {
    fn default() -> Self {
        let universe = Universe::new();
        let world = universe.create_world();

        debug!("Global world with ID {:?} created", world.id());

        // Create channels to send data to and from the global world.
        // At most 1024 events can be queued between server ticks
        let (tx_channel, rx_channel) = channel(1024);
        let mut resources = Resources::default();
        resources.insert(EventRxChannel { rx_channel });

        Multiverse {
            _universe: universe,
            global_world_handle: WorldHandle {
                tx_channel,
                world,
            },
            _instanced_world_handles: HashMap::new(),
            resources,
        }
    }
}

/// Handle for a world.
/// Connections can register their connection by using the `Èvent::RegisterConnection` event.
pub struct WorldHandle {
    pub tx_channel: Sender<Box<Event>>,
    pub world: World,
}

#[cfg(test)]
mod tests {
    use super::super::super::Result;
    use super::super::event::Event;
    use super::*;
    use tokio::sync::mpsc::{channel, Receiver, Sender};

    #[test]
    fn test_multiverse_creation() -> Result<()> {
        let mut m = Multiverse::new();
        let (tx, _): (Sender<Box<Event>>, Receiver<Box<Event>>) = channel(128);

        match m
            .global_world_handle
            .tx_channel
            .try_send(Box::new(Event::RegisterConnection { response_channel: tx }))
        {
            Ok(()) => return Ok(()),
            Err(e) => panic!(e),
        }
    }
}
