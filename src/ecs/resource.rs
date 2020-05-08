/// Module that hold the definitions for Resources used by the ECS.
use crate::ecs::event::EcsEvent;
use async_std::sync::Receiver;
use shipyard::EntityId;

/// Holds the Receiver channel of a world.
pub struct EventRxChannel {
    pub channel: Receiver<EcsEvent>,
}

/// Holds a list with EntityIds marked for deletion.
#[derive(Clone)]
pub struct DeletionList(pub Vec<EntityId>);

pub struct WorldId(pub u64);
