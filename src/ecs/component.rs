/// Module holds the components that the ECS use.
use crate::ecs::event::EcsEvent;
use crate::model::Region;
use async_std::sync::Sender;
use std::time::Instant;

/// Tracks the connection and login information of an user.
#[derive(Clone, Debug)]
pub struct Connection {
    pub channel: Sender<EcsEvent>,
    pub is_version_checked: bool,
    pub is_authenticated: bool,
    pub last_pong: Instant,
    pub waiting_for_pong: bool,
}

/// Holds the account information attached to a connection entity once it's authenticated.
#[derive(Clone, Copy, Debug)]
pub struct Account {
    pub id: i64,
    pub region: Region,
}

/// Holds the configuration settings of a user that are needed at runtime.
pub struct Settings {
    pub visibility_range: u32,
}
