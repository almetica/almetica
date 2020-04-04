/// Module that hold the definitions for Resources used by the ECS.
use std::collections::HashMap;

use crate::ecs::component::SingleEvent;

use legion::entity::Entity;
use tokio::sync::mpsc::{Receiver, Sender};

/// Holds the Receiver channel of a world.
// We use an arc to not copy the event data between the threads.
pub struct EventRxChannel {
    pub channel: Receiver<SingleEvent>,
}

/// Holds the uid to response channel mapping
pub struct ConnectionMapping {
    pub map: HashMap<Entity, Sender<SingleEvent>>,
}
