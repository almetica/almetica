use crate::config::Configuration;
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

/// The local world manager handles the lifecycle of a local world.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::component::GlobalConnection;
    use crate::ecs::message::Message;
    use crate::model::entity::{Account, User};
    use crate::model::repository::{account, user};
    use crate::model::tests::db_test;
    use crate::model::{Class, Gender, PasswordHashAlgorithm, Race};
    use crate::Result;
    use async_std::sync::{channel, Receiver};
    use chrono::{TimeZone, Utc};
    use sqlx::PgPool;
    use std::time::Instant;

    async fn setup(
        pool: PgPool,
    ) -> Result<(
        World,
        EntityId,
        Receiver<EcsMessage>,
        Account,
        User,
        EntityId,
    )> {
        let mut conn = pool.acquire().await?;
        let conf = Configuration::default();
        let (tx_channel, rx_channel) = channel(1024);

        let world = World::new();
        world.add_unique(pool.clone());
        world.add_unique(conf.clone());
        world.add_unique(GlobalMessageChannel {
            channel: tx_channel.clone(),
        });
        world.add_unique(DeletionList(Vec::default()));

        let account = account::create(
            &mut conn,
            &Account {
                id: -1,
                name: "testaccount".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?;

        let user = user::create(
            &mut conn,
            &User {
                id: -1,
                account_id: account.id,
                name: "name-1".to_string(),
                gender: Gender::Male,
                race: Race::Human,
                class: Class::Warrior,
                shape: vec![],
                details: vec![],
                appearance: Default::default(),
                appearance2: 0,
                level: 0,
                awakening_level: 0,
                laurel: 0,
                achievement_points: 0,
                playtime: 0,
                rest_bonus_xp: 0,
                show_face: false,
                show_style: false,
                lobby_slot: 1,
                is_new_character: false,
                tutorial_state: 0,
                is_deleting: false,
                delete_at: None,
                last_logout_at: Utc.ymd(2007, 7, 8).and_hms(9, 10, 11),
                created_at: Utc.ymd(2009, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?;

        let connection_global_world_id = world.run(
            |mut entities: EntitiesViewMut,
             mut connections: ViewMut<GlobalConnection>,
             mut spawns: ViewMut<GlobalUserSpawn>| {
                let id = entities.add_entity(
                    &mut connections,
                    GlobalConnection {
                        channel: tx_channel.clone(),
                        is_version_checked: false,
                        is_authenticated: false,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                );
                entities.add_component(
                    &mut spawns,
                    GlobalUserSpawn {
                        user_id: user.id,
                        account_id: account.id,
                        status: UserSpawnStatus::Waiting,
                        zone_id: 0,
                        connection_local_world_id: None,
                        local_world_id: None,
                        local_world_channel: None,
                        marked_for_deletion: false,
                        is_alive: false,
                    },
                    id,
                );
                id
            },
        );

        let local_world_id = world.run(
            |mut entities: EntitiesViewMut, mut local_worlds: ViewMut<LocalWorld>| {
                let local_world_id = entities.add_entity((), ());
                let mut local_world = ecs::world::LocalWorld::new(
                    &conf,
                    &pool.clone(),
                    local_world_id,
                    tx_channel.clone(),
                );
                let local_world_channel = local_world.channel.clone();
                let join_handle = task::spawn_blocking(move || {
                    local_world.run();
                    Ok(())
                });
                let mut users = HashSet::new();
                users.insert(connection_global_world_id);
                entities.add_component(
                    &mut local_worlds,
                    LocalWorld {
                        instance_type: LocalWorldType::Field,
                        channel_num: None,
                        zone_id: 0,
                        channel: local_world_channel.clone(),
                        join_handle,
                        users,
                        deadline: None,
                    },
                    local_world_id,
                );
                local_world_id
            },
        );

        Ok((
            world,
            connection_global_world_id,
            rx_channel,
            account,
            user,
            local_world_id,
        ))
    }

    #[test]
    fn test_local_world_loaded_success() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel, _account, _user, local_world_id) =
                task::block_on(async { setup(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::LocalWorldLoaded {
                            successful: true,
                            global_world_id: local_world_id,
                        }),
                    );
                },
            );

            world.run(local_world_manager_system);

            let spawns = world.borrow::<View<GlobalUserSpawn>>();
            let spawn = spawns.get(connection_global_world_id);
            assert_eq!(spawn.status, UserSpawnStatus::CanSpawn);

            Ok(())
        })
    }

    #[test]
    fn test_local_world_loaded_failure() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel, _account, _user, local_world_id) =
                task::block_on(async { setup(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::LocalWorldLoaded {
                            successful: false,
                            global_world_id: local_world_id,
                        }),
                    );
                },
            );

            world.run(local_world_manager_system);

            let spawns = world.borrow::<View<GlobalUserSpawn>>();
            let spawn = spawns.get(connection_global_world_id);
            assert_eq!(spawn.status, UserSpawnStatus::SpawnFailed);

            Ok(())
        })
    }
}

// TODO TEST  UserSpawnStatus::Requesting
// TODO TEST  spawn.marked_for_deletion == true
// TODO TEST world.deadline.is_some() && world.deadline.unwrap() <= now
