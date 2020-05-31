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

const LOCAL_WORLD_IDLE_LIFETIME_SEC: u64 = 300;

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
            deletion_list.0.push(connection_global_world_id);
            info!(
                "Marked global user {:?} for deletion",
                connection_global_world_id
            );
            if let Err(e) =
                handle_user_despawn(&spawn, connection_global_world_id, &mut local_worlds)
            {
                // TODO decide how to handle an error while de-spawning an user
                id_span!(connection_global_world_id);
                error!("Can't de-spawn user: {:?}", e)
            };
        }
    }

    // Delete local worlds that don't have any users and passed their deadline.
    let now = Instant::now();
    local_worlds
        .iter()
        .with_id()
        .filter(|(_id, world)| world.deadline.is_some() && world.deadline.unwrap() < now)
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

    info!(
        "Spawning user {:?} in local world {:?}",
        connection_global_world_id, world_id
    );

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
    local_world.users.remove(&connection_global_world_id);

    if local_world.users.is_empty() {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(LOCAL_WORLD_IDLE_LIFETIME_SEC))
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
    use crate::ecs::dto::UserInitializer;
    use crate::ecs::message::Message;
    use crate::model::entity::{Account, User, UserLocation};
    use crate::model::repository::{account, user};
    use crate::model::tests::db_test;
    use crate::model::{Class, Gender, PasswordHashAlgorithm, Race};
    use crate::Result;
    use async_std::sync::{channel, Receiver, Sender};
    use chrono::{TimeZone, Utc};
    use nalgebra::{Point3, Rotation3, Vector3};
    use sqlx::PgPool;
    use std::ops::Sub;
    use std::time::Instant;

    async fn setup(
        pool: PgPool,
    ) -> Result<(
        World,
        EntityId,
        Sender<EcsMessage>,
        Receiver<EcsMessage>,
        Account,
        User,
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

        Ok((
            world,
            connection_global_world_id,
            tx_channel,
            rx_channel,
            account,
            user,
        ))
    }

    fn create_local_world(
        world: &mut World,
        global_world_channel: &Sender<EcsMessage>,
        conf: &Configuration,
        pool: &PgPool,
        connection_global_world_id: EntityId,
        deadline: Option<Instant>,
    ) -> Result<(EntityId, Sender<EcsMessage>)> {
        world.run(
            |mut entities: EntitiesViewMut, mut local_worlds: ViewMut<LocalWorld>| {
                let local_world_id = entities.add_entity((), ());
                let mut local_world = ecs::world::LocalWorld::new(
                    conf,
                    pool,
                    local_world_id,
                    global_world_channel.clone(),
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
                        deadline,
                    },
                    local_world_id,
                );
                Ok((local_world_id, local_world_channel.clone()))
            },
        )
    }

    #[test]
    fn test_local_world_loaded_success() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (
                    mut world,
                    connection_global_world_id,
                    tx_channel,
                    _rx_channel,
                    _account,
                    _user,
                ) = setup(pool.clone()).await?;

                let (local_world_id, _local_world_channel) = create_local_world(
                    &mut world,
                    &tx_channel,
                    &Configuration::default(),
                    &pool,
                    connection_global_world_id,
                    None,
                )?;

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

                world.run(|spawns: View<GlobalUserSpawn>| {
                    let spawn = spawns.try_get(connection_global_world_id)?;
                    assert_eq!(spawn.status, UserSpawnStatus::CanSpawn);

                    Ok::<(), anyhow::Error>(())
                })?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_local_world_loaded_failure() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (
                    mut world,
                    connection_global_world_id,
                    tx_channel,
                    _rx_channel,
                    _account,
                    _user,
                ) = setup(pool.clone()).await?;

                let (local_world_id, _local_world_channel) = create_local_world(
                    &mut world,
                    &tx_channel,
                    &Configuration::default(),
                    &pool,
                    connection_global_world_id,
                    None,
                )?;

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

                world.run(|spawns: View<GlobalUserSpawn>| {
                    let spawn = spawns.try_get(connection_global_world_id)?;
                    assert_eq!(spawn.status, UserSpawnStatus::SpawnFailed);

                    Ok::<(), anyhow::Error>(())
                })?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_user_requesting_spawn_world_creation() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (world, connection_global_world_id, _tx_channel, _rx_channel, _account, _user) =
                    setup(pool).await?;

                world.run(|mut spawns: ViewMut<GlobalUserSpawn>| {
                    let mut spawn = (&mut spawns).try_get(connection_global_world_id)?;
                    spawn.status = UserSpawnStatus::Requesting;

                    Ok::<(), anyhow::Error>(())
                })?;

                world.run(local_world_manager_system);

                world.run(|worlds: View<LocalWorld>, spawns: View<GlobalUserSpawn>| {
                    assert_eq!(worlds.iter().count(), 1);
                    let world = worlds.iter().next().unwrap();
                    assert_eq!(world.users.len(), 1);
                    assert!(world.deadline.is_none());

                    let spawn = (&spawns).try_get(connection_global_world_id)?;
                    assert!(spawn.local_world_id.is_some());
                    assert!(spawn.local_world_channel.is_some());
                    assert_eq!(spawn.status, UserSpawnStatus::Waiting);

                    Ok::<(), anyhow::Error>(())
                })?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_user_requesting_spawn_world_reuse() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (
                    mut world,
                    connection_global_world_id,
                    tx_channel,
                    _rx_channel,
                    _account,
                    _user,
                ) = setup(pool.clone()).await?;

                let (local_world_id, _local_world_channel) = create_local_world(
                    &mut world,
                    &tx_channel,
                    &Configuration::default(),
                    &pool,
                    connection_global_world_id,
                    Some(Instant::now()),
                )?;

                world.run(|mut spawns: ViewMut<GlobalUserSpawn>| {
                    let mut spawn = (&mut spawns).try_get(connection_global_world_id)?;
                    spawn.status = UserSpawnStatus::Requesting;
                    Ok::<(), anyhow::Error>(())
                })?;

                world.run(local_world_manager_system);

                world.run(|worlds: View<LocalWorld>| {
                    assert_eq!(worlds.iter().count(), 1);
                    let world = worlds.try_get(local_world_id)?;
                    assert_eq!(world.users.len(), 1);
                    assert_eq!(world.deadline, None);

                    Ok::<(), anyhow::Error>(())
                })?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_user_despawn() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (mut world, connection_global_world_id, tx_channel, rx_channel, _account, user) =
                    setup(pool.clone()).await?;

                let (local_world_id, local_world_channel) = create_local_world(
                    &mut world,
                    &tx_channel,
                    &Configuration::default(),
                    &pool,
                    connection_global_world_id,
                    Some(Instant::now()),
                )?;

                world.run(|mut spawns: ViewMut<GlobalUserSpawn>| {
                    let mut spawn = (&mut spawns).try_get(connection_global_world_id)?;
                    spawn.status = UserSpawnStatus::Requesting;

                    Ok::<(), anyhow::Error>(())
                })?;
                world.run(local_world_manager_system);

                // We need to flush the global channel
                rx_channel.recv().await?;
                assert!(rx_channel.is_empty());

                world.run(|connections: View<GlobalConnection>| {
                    let connection = (&connections).try_get(connection_global_world_id).unwrap();
                    send_message(
                        Box::new(Message::PrepareUserSpawn {
                            user_initializer: UserInitializer {
                                connection_global_world_id,
                                connection_channel: connection.channel.clone(),
                                user,
                                location: UserLocation {
                                    user_id: 0,
                                    zone_id: 0,
                                    point: Point3::new(1.0, 1.0, 1.0),
                                    rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 0.0),
                                },
                                is_alive: true,
                            },
                        }),
                        &local_world_channel,
                    );
                });

                let connection_local_world_id = match &*rx_channel.recv().await? {
                    Message::UserSpawnPrepared {
                        connection_local_world_id,
                        ..
                    } => connection_local_world_id.clone(),
                    _ => panic!("Couldn't find Message::UserSpawnPrepared"),
                };

                world.run(|mut spawns: ViewMut<GlobalUserSpawn>| {
                    let mut spawn = (&mut spawns).try_get(connection_global_world_id)?;
                    spawn.connection_local_world_id = Some(connection_local_world_id);
                    spawn.status = UserSpawnStatus::Spawned;
                    spawn.marked_for_deletion = true;

                    Ok::<(), anyhow::Error>(())
                })?;
                world.run(local_world_manager_system);

                world.run(
                    |worlds: View<LocalWorld>, mut deletion_list: UniqueViewMut<DeletionList>| {
                        assert_eq!(worlds.iter().count(), 1);
                        let world = worlds.try_get(local_world_id)?;

                        assert_eq!(world.users.len(), 0);
                        assert!(world.deadline.is_some());

                        assert_eq!(deletion_list.0.pop(), Some(connection_global_world_id));

                        Ok::<(), anyhow::Error>(())
                    },
                )?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_delete_unused_local_worlds() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (
                    mut world,
                    connection_global_world_id,
                    tx_channel,
                    _rx_channel,
                    _account,
                    _user,
                ) = setup(pool.clone()).await?;

                let (local_world_id, _local_world_channel) = create_local_world(
                    &mut world,
                    &tx_channel,
                    &Configuration::default(),
                    &pool,
                    connection_global_world_id,
                    Some(Instant::now()),
                )?;

                world.run(|mut worlds: ViewMut<LocalWorld>| {
                    let mut world = (&mut worlds).try_get(local_world_id)?;
                    world.deadline = Some(Instant::now().sub(Duration::from_secs(1)));
                    world.users.clear();

                    Ok::<(), anyhow::Error>(())
                })?;

                world.run(local_world_manager_system);

                world.run(|mut deletion_list: UniqueViewMut<DeletionList>| {
                    assert_eq!(deletion_list.0.len(), 1);
                    assert_eq!(deletion_list.0.pop(), Some(local_world_id));

                    Ok::<(), anyhow::Error>(())
                })?;

                Ok(())
            })
        })
    }
}
