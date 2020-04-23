/// Module that hold the definitions for Resources used by the ECS.
use std::collections::HashMap;

use shipyard::EntityId;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::ecs::event::EcsEvent;

/// Holds the Receiver channel of a world.
pub struct EventRxChannel {
    pub channel: Receiver<EcsEvent>,
}

/// Holds the Entity to response channel mapping
pub struct ConnectionMapping(pub HashMap<EntityId, Sender<EcsEvent>>);

/// Holds a list with EntityIds marked for deletion.
#[derive(Clone)]
pub struct DeletionList(pub Vec<EntityId>);

pub struct WorldId(pub u64);
