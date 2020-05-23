/// Module that holds data structures used by the ECS to transfer data.
use crate::ecs::message::EcsMessage;
use crate::model::entity;
use async_std::sync::Sender;
use shipyard::EntityId;

#[derive(Clone, Debug)]
pub struct UserInitializer {
    pub connection_global_world_id: EntityId,
    pub connection_channel: Sender<EcsMessage>,
    pub user: entity::User,
}
