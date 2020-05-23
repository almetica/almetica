/// Module that hold the definitions for Resources used by the ECS.
use crate::ecs::message::EcsMessage;
use async_std::sync::{Receiver, Sender};
use shipyard::EntityId;

/// Holds the Receiver channel of a world.
pub struct InputChannel {
    pub channel: Receiver<EcsMessage>,
}

/// Holds the Sender channel of the global world.
pub struct GlobalMessageChannel {
    pub channel: Sender<EcsMessage>,
}

/// Holds a list with EntityIds marked for deletion.
#[derive(Clone)]
pub struct DeletionList(pub Vec<EntityId>);

pub struct ShutdownSignal {
    pub status: ShutdownSignalStatus,
}

#[derive(PartialEq)]
pub enum ShutdownSignalStatus {
    Operational,
    ShutdownInProgress,
    Shutdown,
}
