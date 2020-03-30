/// Module that handles the world generation and handling
use std::collections::HashMap;

use super::event::Event;
use super::resource::EventRxChannel;

use legion::prelude::*;
use tokio::sync::mpsc::{Sender, Receiver, channel};

/// Holds the ECS for the global world and all instanced worlds.
struct Multiverse {
    pub universe: Universe,
    pub global_world_handle: WorldHandle,
    pub instanced_world_handles: HashMap<String, World>,
}

impl Multiverse {
    /// Creates a new Multiverse.
    pub fn new() -> Multiverse {
        let universe = Universe::new();
        let mut global = universe.create_world();

        // Create channels to send data to and from the global world.
        // At most 1024 events can be queued between server ticks
        let (tx, rx): (Sender<Box<Event>>, Receiver<Box<Event>>) = channel(1024);
        global.resources.insert(EventRxChannel{rx_channel: rx});

        Multiverse {
            universe: universe,
            global_world_handle: WorldHandle {
                tx_channel: tx,
                world: global,
            },
            instanced_world_handles: HashMap::new(),
        }
    }
}

/// Handle for a world.
/// Connections can register their connection by using the `Èvent::RegisterConnection` event.
struct WorldHandle {
    pub tx_channel: Sender<Box<Event>>,
    pub world: World,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::event::Event;
    use super::super::super::Result;
    use tokio::sync::mpsc::{Sender, Receiver, channel};

    #[test]
    fn test_multiverse_creation() -> Result<()> {
        let mut m = Multiverse::new();
        let (tx, _): (Sender<Box<Event>>, Receiver<Box<Event>>) = channel(128);

        match m.global_world_handle.tx_channel.try_send(Box::new(Event::RegisterConnection{tx_channel: tx})) {
            Ok(()) => return Ok(()),
            Err(e) => panic!(e),
        }
    }
}
