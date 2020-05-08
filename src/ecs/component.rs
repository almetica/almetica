/// Module holds the components that the ECS use.
use crate::ecs::event::EcsEvent;
use crate::model::Region;
use async_std::sync::Sender;
use shipyard::EntityId;
use std::time::Instant;

/// Tracks the connection and login information of an user.
pub struct Connection {
    pub channel: Sender<EcsEvent>,
    pub verified: bool,
    pub version_checked: bool,
    pub region: Option<Region>,
    pub last_pong: Instant,
    pub waiting_for_pong: bool,
}

/// Holds the connection entity id from the global world for using in a local world.
pub struct ConnectionID(pub EntityId);

/// Holds the configuration settings of a user that are needed at runtime.
pub struct Settings {
    pub visibility_range: u32,
}
