/// Module that hold the definitions for Resources used by the ECS.
use super::event::Event;

use tokio::sync::mpsc::Receiver;

/// Holds the Receiver channel of a world.
pub struct EventRxChannel {
    pub rx_channel: Receiver<Box<Event>>,
}
