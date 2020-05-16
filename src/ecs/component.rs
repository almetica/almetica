/// Module holds the components that the ECS use.
use crate::ecs::event::EcsEvent;
use crate::model::Region;
use async_std::sync::Sender;
use shipyard::EntityId;
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

/// Holds the spawn information of an user.
pub struct UserSpawn {
    pub status: UserSpawnStatus,
    pub zone_id: i32,
    pub local_world_id: Option<EntityId>,
}

pub enum UserSpawnStatus {
    Waiting,  // Used when the LocalWorld the user wants do spawn yet isn't created/loaded yet.
    Spawning, // Signals that the user is currently in the process of spawning
    Spawned,  // User is fully spawned
}

/// Holds information about a local world.
pub struct LocalWorld {
    pub instance_type: LocalWorldType,
    pub channel_num: Option<i32>,
    pub zone_id: i32,
    pub channel: Sender<EcsEvent>,
}

pub enum LocalWorldType {
    Arena,   // PVP Arena
    Dungeon, // Instanced Dungeons / Raids
    Field,   // Fields / Cities
}
