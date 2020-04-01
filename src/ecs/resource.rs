/// Module that hold the definitions for Resources used by the ECS.
use std::collections::HashMap;
use std::sync::Arc;

use crate::ecs::event::Event;
use tokio::sync::mpsc::{Receiver, Sender};

/// Holds the Receiver channel of a world.
// We use an arc to not copy the event data between the threads.
pub struct EventRxChannel {
    pub channel: Receiver<Arc<Event>>,
}

/// Holds the UID to response channel mapping
pub struct ConnectionMapping {
    pub map: HashMap<u64, Sender<Arc<Event>>>,
}
