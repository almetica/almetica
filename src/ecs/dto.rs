/// Module that holds data structures used by the ECS to transfer data.
use crate::ecs::message::EcsMessage;
use crate::model::entity;
use crate::model::entity::UserLocation;
use async_std::sync::Sender;
use shipyard::EntityId;

/// Used to send data from the Global World to the Local World when spawning an user.
#[derive(Clone, Debug)]
pub struct UserInitializer {
    pub connection_global_world_id: EntityId,
    pub connection_channel: Sender<EcsMessage>,
    pub user: entity::User,
    pub location: UserLocation,
    pub is_alive: bool,
}

/// Used to send data from the Local World to the Global World when de-spawning an user.
#[derive(Clone, Debug)]
pub struct UserFinalizer {
    pub user_id: i32,
    pub location: UserLocation,
    pub is_alive: bool,
}
