/// Module holds the components that the ECS use.
use crate::ecs::message::EcsMessage;
use crate::model::Region;
use crate::Result;
use async_std::sync::Sender;
use async_std::task::JoinHandle;
use shipyard::EntityId;
use std::collections::HashSet;
use std::time::Instant;

/// Tracks the connection and login information of a player for the global world.
#[derive(Clone, Debug)]
pub struct GlobalConnection {
    pub channel: Sender<EcsMessage>,
    pub is_version_checked: bool,
    pub is_authenticated: bool,
    pub last_pong: Instant,
    pub waiting_for_pong: bool,
}

/// Tracks the connection of a player for a local world.
#[derive(Clone, Debug)]
pub struct LocalConnection {
    pub channel: Sender<EcsMessage>,
}

/// Holds the account information attached to a connection entity once it's authenticated.
#[derive(Clone, Copy, Debug)]
pub struct Account {
    pub id: i64,
    pub region: Region,
}

/// Holds the configuration settings of a user that are needed at runtime.
#[derive(Clone, Debug)]
pub struct Settings {
    pub visibility_range: u32,
}

/// Holds the global spawn information of an user.
#[derive(Clone, Debug)]
pub struct GlobalUserSpawn {
    pub user_id: i32,
    pub account_id: i64,
    pub status: UserSpawnStatus,
    pub zone_id: i32,
    pub connection_local_world_id: Option<EntityId>,
    pub local_world_id: Option<EntityId>,
    pub local_world_channel: Option<Sender<EcsMessage>>,
    pub marked_for_deletion: bool,
    pub is_alive: bool,
}

/// Holds the local spawn information of an user.
#[derive(Clone, Debug)]
pub struct LocalUserSpawn {
    pub user_id: i32,
    pub account_id: i64,
    pub status: UserSpawnStatus,
    pub is_alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserSpawnStatus {
    Requesting,  // Requests to be spawned.
    Waiting,     // Spawn request acknowledged but instance is being created.
    CanSpawn,    // Signals the user spawner that the instance can now accept user spawns
    Spawning,    // User has been given the command to spawn.
    Spawned,     // User is spawned in a local world.
    SpawnFailed, // Spawn wasn't successful
}

/// Holds information about a local world.
#[derive(Debug)]
pub struct LocalWorld {
    pub instance_type: LocalWorldType,
    pub channel_num: Option<i32>,
    pub zone_id: i32,
    pub channel: Sender<EcsMessage>,
    pub join_handle: JoinHandle<Result<()>>,
    pub users: HashSet<EntityId>,  // connection_global_world_id
    pub deadline: Option<Instant>, // Set when no users are present
}

#[derive(Clone, Debug, PartialEq)]
pub enum LocalWorldType {
    Arena,   // PVP Arena
    Dungeon, // Instanced Dungeons / Raids
    Field,   // Fields / Cities
}
