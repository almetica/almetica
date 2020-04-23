/// Module holds the components that the ECS use.
use std::time::Instant;

use shipyard::EntityId;

use crate::ecs::event::EcsEvent;
use crate::model::Region;

/// Incoming event.
pub struct IncomingEvent(pub EcsEvent);

/// Outgoing event.
pub struct OutgoingEvent(pub EcsEvent);

/// Tracks the connection and login information of an user.
pub struct Connection {
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
