use crate::config::Configuration;
/// The local world manager handles the lifecycle of a local world.
use crate::ecs::component::{
    GlobalConnection, GlobalUserSpawn, LocalWorld, LocalWorldType, UserSpawnStatus,
};
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::resource::{DeletionList, GlobalMessageChannel};
use crate::ecs::system::send_message;
use crate::{ecs, Result};
use anyhow::{ensure, Context};
use async_std::task;
use shipyard::*;
use sqlx::PgPool;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, info_span};

const LOCAL_WORLD_IDLE_LIFETIME_SEC: u64 = 15;

// TODO write tests for the local_world_manager_system
pub fn local_world_manager_system(
    incoming_messages: View<EcsMessage>,
    _connections: View<GlobalConnection>,
    mut user_spawns: ViewMut<GlobalUserSpawn>,
    mut local_worlds: ViewMut<LocalWorld>,
    mut entities: EntitiesViewMut,
    config: UniqueView<Configuration>,
    pool: UniqueView<PgPool>,
    global_world_channel: UniqueView<GlobalMessageChannel>,
    mut deletion_list: UniqueViewMut<DeletionList>,
) {
    (&incoming_messages)
        .iter()
        .for_each(|message| match &**message {
            Message::LocalWorldLoaded {
                successful,
                global_world_id,
            } => {
                if let Err(e) = handle_local_world_loaded(
                    *successful,
                    *global_world_id,
                    &mut user_spawns,
                    &mut local_worlds,
                    &mut deletion_list,
                ) {
                    error!("Ignoring Message::LocalWorldLoaded: {:?}", e)
                }
            }
            _ => { /* Ignore all other messages */ }
        });

    // Look for users that either want to spawn or are marked for deletion.
    for (connection_global_world_id, spawn) in (&mut user_spawns).iter().with_id() {
        if spawn.status == UserSpawnStatus::Requesting {
            if let Err(e) = handle_user_requesting_spawn(
                spawn,
                connection_global_world_id,
                &mut local_worlds,
                &mut entities,
                &config,
                &global_world_channel,
                &pool,
            ) {
                // TODO decide how to handle an error while requesting a user spawn
                id_span!(connection_global_world_id);
                error!("Can't handle user request to spawn: {:?}", e)
            }
        }
        if spawn.marked_for_deletion {
            if let Err(e) =
                handle_user_despawn(&spawn, connection_global_world_id, &mut local_worlds)
            {
                // TODO decide how to handle an error while de-spawning an user
                id_span!(connection_global_world_id);
                error!("Can't de-spawn user: {:?}", e)
            }
        }
    }

    // Delete local worlds that don't have any users and passed their deadline.
    let now = Instant::now();
    local_worlds
        .iter()
        .with_id()
        .filter(|(_id, world)| world.deadline.is_some() && world.deadline.unwrap() <= now)
        .for_each(|(id, world)| {
            send_message(assemble_shutdown_message(), &world.channel);
            deletion_list.0.push(id);
            info!("Marked local world {:?} for deletion", id);
        });
}

fn handle_user_requesting_spawn(
    mut spawn: &mut GlobalUserSpawn,
    connection_global_world_id: EntityId,
    local_worlds: &mut ViewMut<LocalWorld>,
    entities: &mut EntitiesViewMut,
    config: &UniqueView<Configuration>,
    global_world_channel: &UniqueView<GlobalMessageChannel>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    // TODO once we implement parties / dungeons / pvp arenas, this code needs to be extended
    let (world_id, channel) = if let Some((world_id, world)) = local_worlds
        .iter()
        .with_id()
        .filter(|(_id, world)| world.zone_id == spawn.zone_id)
        .next()
    {
        info!(
            "Spawning user {:?} in local world {:?}",
            connection_global_world_id, world_id
        );
        world.users.insert(connection_global_world_id);
        world.deadline = None;

        (world_id, world.channel.clone())
    } else {
        // TODO once we have implemented the datacenter parser, we need to extend this part
        let world_id = entities.add_entity((), ());
        let mut local_world = ecs::world::LocalWorld::new(
            &**config.clone(),
            &**pool.clone(),
            world_id,
            global_world_channel.channel.clone(),
        );
        let local_world_channel = local_world.channel.clone();
        let join_handle = task::spawn_blocking(move || {
            local_world.run();
            Ok(())
        });

        let mut users = HashSet::new();
        users.insert(connection_global_world_id);

        entities.add_component(
            local_worlds,
            LocalWorld {
                instance_type: LocalWorldType::Field,
                channel_num: None,
                zone_id: spawn.zone_id,
                channel: local_world_channel.clone(),
                join_handle,
                users,
                deadline: None,
            },
            world_id,
        );

        (world_id, local_world_channel)
    };

    spawn.local_world_id = Some(world_id);
    spawn.local_world_channel = Some(channel);
    spawn.status = UserSpawnStatus::Waiting;
    Ok(())
}

fn handle_user_despawn(
    spawn: &GlobalUserSpawn,
    connection_global_world_id: EntityId,
    local_worlds: &mut ViewMut<LocalWorld>,
) -> Result<()> {
    ensure!(
        spawn.connection_local_world_id.is_some(),
        "Local world ID is not set for user spawn {:?}",
        connection_global_world_id
    );

    ensure!(
        spawn.local_world_channel.is_some(),
        "Local world channel is not set for user spawn {:?}",
        connection_global_world_id
    );

    ensure!(
        spawn.local_world_id.is_some(),
        "Can't find the ID of the local world for user spawn {:?}",
        connection_global_world_id
    );

    send_message(
        assemble_user_despawn(spawn.connection_local_world_id.unwrap()),
        &spawn.local_world_channel.clone().unwrap(),
    );

    // Remove user from the local world users list and set the deadline if there are no users left on the local world
    let mut local_world = local_worlds
        .try_get(spawn.local_world_id.unwrap())
        .context("Can't find the local world")?;
    local_world
        .users
        .remove(&spawn.connection_local_world_id.unwrap());

    if local_world.users.is_empty() {
        let deadline = Instant::now()
            .checked_sub(Duration::from_secs(LOCAL_WORLD_IDLE_LIFETIME_SEC))
            .unwrap();
        local_world.deadline = Some(deadline);
    }

    Ok(())
}

// TODO use a type alias for the EntityID to differentiate between "local world id" and "global world id"
fn handle_local_world_loaded(
    successful: bool,
    global_world_id: EntityId,
    user_spawns: &mut ViewMut<GlobalUserSpawn>,
    local_worlds: &mut ViewMut<LocalWorld>,
    deletion_list: &mut UniqueViewMut<DeletionList>,
) -> Result<()> {
    debug!("Message::LocalWorldLoaded incoming");

    let world = local_worlds
        .try_get(global_world_id)
        .context(format!("Can't find local world {:?}", global_world_id))?;

    for user_id in &world.users {
        let spawn = (user_spawns)
            .try_get(*user_id)
            .context(format!("Can't find user {:?}", user_id))?;

        spawn.status = if successful {
            UserSpawnStatus::CanSpawn
        } else {
            UserSpawnStatus::SpawnFailed
        }
    }

    // The local world didn't loaded successful, so delete it's global world entity
    if !successful {
        deletion_list.0.push(global_world_id);
    }

    Ok(())
}

fn assemble_shutdown_message() -> EcsMessage {
    Box::new(Message::ShutdownSignal { forced: false })
}

fn assemble_user_despawn(connection_local_world_id: EntityId) -> EcsMessage {
    Box::new(Message::UserDespawn {
        connection_local_world_id,
    })
}
