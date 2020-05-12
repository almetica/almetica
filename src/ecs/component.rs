/// Module holds the components that the ECS use.
use crate::ecs::event::EcsEvent;
use crate::model::Region;
use async_std::sync::Sender;
use std::time::Instant;

/// Tracks the connection and login information of an user.
pub struct Connection {
    pub channel: Sender<EcsEvent>,
    pub version_checked: bool,
    pub region: Option<Region>,
    pub last_pong: Instant,
    pub waiting_for_pong: bool,
}

/// Holds the account ID that is attached to a connection entity once it's authenticated.
pub struct AccountID(pub i64);

/// Holds the configuration settings of a user that are needed at runtime.
pub struct Settings {
    pub visibility_range: u32,
}
