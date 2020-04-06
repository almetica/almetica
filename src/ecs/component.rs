/// Module holds the components that the ECS use.
use std::sync::Arc;
use std::time::Instant;

use crate::ecs::event::Event;
use crate::model::Region;

/// A single event emitted inside the ECS.
pub type SingleEvent = Arc<Event>;

/// A batch event. Mainly used to send packets in a special order to the client.
pub type BatchEvent = Vec<Arc<Event>>;

/// Tracks the connection and login information of an user.
pub struct Connection {
    pub verified: bool,
    pub version_checked: bool,
    pub region: Option<Region>,
    pub last_pong: Instant,
}

// Holds the configuration settings of a user that are needed at runtime
pub struct Settings {
    pub visibility_range: u32,
}
