use crate::ecs::component::{GlobalConnection, GlobalUserSpawn, UserSpawnStatus};
use crate::ecs::dto::UserInitializer;
use crate::ecs::message::Message::{
    PrepareUserSpawn, RegisterLocalWorld, ResponseLoadHint, ResponseLoadTopo, ResponseLogin,
    UserReadyToConnect,
};
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::system::global::send_message_to_connection;
use crate::ecs::system::send_message;
use crate::model::repository::{user, user_location};
use crate::model::{entity, TemplateID, Vec3f};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::{bail, ensure, Context};
use async_std::sync::Sender;
use async_std::task;
use shipyard::*;
use sqlx::PgPool;
use tracing::{debug, error, info_span};

/// Handles the global spawn process.
pub fn user_spawner_system(
    incoming_messages: View<EcsMessage>,
    connections: View<GlobalConnection>,
    mut spawns: ViewMut<GlobalUserSpawn>,
    entities: EntitiesView,
    pool: UniqueView<PgPool>,
) {
    (&incoming_messages)
        .iter()
        .for_each(|message| match &**message {
            Message::RequestSelectUser {
                connection_global_world_id,
                account_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_select_user(
                    packet,
                    *connection_global_world_id,
                    *account_id,
                    &mut spawns,
                    &entities,
                    &pool,
                ) {
                    error!("Ignoring select user request: {:?}", e);
                }
            }
            Message::UserSpawnPrepared {
                connection_global_world_id,
                connection_local_world_id,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_user_spawn_prepared(
                    *connection_global_world_id,
                    *connection_local_world_id,
                    &mut spawns,
                    &connections,
                    &pool,
                ) {
                    error!("Ignoring user spawn prepared message: {:?}", e);
                }
            }
            Message::UserSpawned {
                connection_global_world_id,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_user_spawned(*connection_global_world_id, &mut spawns) {
                    error!("Ignoring user spawned message: {:?}", e);
                }
            }
            _ => { /* Ignore all other messages */ }
        });

    for (connection_global_world_id, spawn) in spawns.iter().with_id().filter(|(_id, spawn)| {
        spawn.status == UserSpawnStatus::CanSpawn || spawn.status == UserSpawnStatus::SpawnFailed
    }) {
        if spawn.status == UserSpawnStatus::CanSpawn {
            id_span!(connection_global_world_id);
            if let Err(e) =
                prepare_local_spawn(spawn, connection_global_world_id, &connections, &pool)
            {
                error!("Can't prepare local spawn: {:?}", e);
            }
        } else if spawn.status == UserSpawnStatus::SpawnFailed {
            // FIXME we don't want to panic here, but right now I don't know how to properly handle this error
            id_span!(connection_global_world_id);
            error!("Spawn failed for user {:?}", connection_global_world_id);
            panic!("SPAWN FAILED");
        }
    }
}

fn prepare_local_spawn(
    spawn: &GlobalUserSpawn,
    connection_global_world_id: EntityId,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    ensure!(
        spawn.local_world_channel.is_some(),
        "Local world channel is not set"
    );

    let connection = connections
        .try_get(connection_global_world_id)
        .context("Can't find connection component")?;

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        let user = user::get_by_id(&mut conn, spawn.user_id).await?;
        let location = user_location::get_by_user_id(&mut conn, spawn.user_id).await?;
        send_message(
            assemble_prepare_user_spawn(
                connection_global_world_id,
                connection.channel.clone(),
                user,
                location,
            ),
            &spawn.local_world_channel.clone().unwrap(),
        );

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_user_spawned(
    connection_global_world_id: EntityId,
    spawns: &mut ViewMut<GlobalUserSpawn>,
) -> Result<()> {
    debug!("Message::UserSpawned incoming");

    let mut spawn = spawns.try_get(connection_global_world_id).context(format!(
        "Can't get user spawn component {:?}",
        connection_global_world_id
    ))?;
    spawn.status = UserSpawnStatus::Spawned;
    Ok(())
}

fn handle_select_user(
    packet: &CSelectUser,
    connection_global_world_id: EntityId,
    account_id: i64,
    spawns: &mut ViewMut<GlobalUserSpawn>,
    entities: &EntitiesView,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestSelectUser incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        let user = user::get_by_id(&mut conn, packet.database_id).await?;
        ensure!(
            user.account_id == account_id,
            "User {:?} doesn't belongs to account {:?}",
            user,
            account_id
        );

        if let Ok(spawn) = spawns.try_get(connection_global_world_id) {
            bail!("Account is already logged in with user {}", spawn.user_id);
        }

        // TODO implement the user_location model and use it here

        entities.add_component(
            spawns,
            GlobalUserSpawn {
                connection_local_world_id: None,
                user_id: user.id,
                account_id,
                status: UserSpawnStatus::Requesting,
                zone_id: 0,
                local_world_id: None,
                local_world_channel: None,
                marked_for_deletion: false,
                is_alive: true,
            },
            connection_global_world_id,
        );

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_user_spawn_prepared(
    connection_global_world_id: EntityId,
    connection_local_world_id: EntityId,
    spawns: &mut ViewMut<GlobalUserSpawn>,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::UserSpawnPrepared incoming");

    let mut spawn = spawns.try_get(connection_global_world_id).context(format!(
        "Can't get user spawn component {:?}",
        connection_global_world_id
    ))?;
    spawn.connection_local_world_id = Some(connection_local_world_id);
    spawn.status = UserSpawnStatus::Waiting;

    ensure!(
        spawn.local_world_channel.is_some(),
        "Local world channel is not set"
    );

    let connection = connections
        .try_get(connection_global_world_id)
        .context(format!(
            "Can't get user connection component {:?}",
            connection_global_world_id
        ))?;

    // Register the local world with the connection and send the ResponseLogin
    send_message(
        assemble_register_local_world(
            spawn.connection_local_world_id.unwrap(),
            spawn.local_world_channel.clone().unwrap(),
        ),
        &connection.channel,
    );

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        let user = user::get_by_id(&mut conn, spawn.user_id)
            .await
            .context(format!("Can't query user {}", spawn.user_id))?;

        send_message_to_connection(
            assemble_response_login(connection_global_world_id, user),
            connections,
        );

        // TODO Send all other persisted date

        // TODO use the user_location entity once implemented
        send_message_to_connection(
            assemble_response_load_topo(connection_global_world_id),
            connections,
        );
        send_message_to_connection(
            assemble_response_load_hint(connection_global_world_id),
            connections,
        );

        // Tell the local world that a user could connect to it soon
        send_message(
            assemble_user_ready_to_connect(spawn.connection_local_world_id.unwrap()),
            &spawn.local_world_channel.clone().unwrap(),
        );

        Ok::<(), anyhow::Error>(())
    })?)
}

fn assemble_register_local_world(
    connection_local_world_id: EntityId,
    local_world_channel: Sender<EcsMessage>,
) -> EcsMessage {
    Box::new(RegisterLocalWorld {
        connection_local_world_id,
        local_world_channel,
    })
}

fn assemble_response_login(connection_global_world_id: EntityId, user: entity::User) -> EcsMessage {
    Box::new(ResponseLogin {
        connection_global_world_id,
        account_id: user.account_id,
        user_id: user.id,
        packet: SLogin {
            servants: vec![],
            name: user.name,
            details: user.details,
            shape: user.shape,
            template_id: TemplateID {
                race: user.race,
                gender: user.gender,
                class: user.class,
            },
            id: connection_global_world_id,
            server_id: 1,
            db_id: user.id,
            action_mode: 0,
            alive: true,
            status: 0,
            walk_speed: 50,
            run_speed: 150,
            appearance: user.appearance,
            visible: true,
            is_second_character: false,
            level: 1,
            awakening_level: 0,
            profession_mineral: 0,
            profession_bug: 0,
            profession_herb: 0,
            profession_energy: 0,
            profession_pet: 0,
            pvp_declared_count: 0,
            pvp_kill_count: 0,
            total_exp: 0,
            level_exp: 0,
            total_level_exp: 0,
            ep_level: 0,
            ep_exp: 0,
            ep_daily_exp: 0,
            rest_bonus_exp: 0,
            max_rest_bonus_exp: 0,
            exp_bonus_percent: 1.0,
            drop_bonus_percent: 0.0,
            weapon: 0,
            body: 0,
            hand: 0,
            feet: 0,
            underwear: 0,
            head: 0,
            face: 0,
            server_time: 37990571,
            is_pvp_server: true,
            chat_ban_end_time: 0,
            title: 0,
            weapon_model: 0,
            body_model: 0,
            hand_model: 0,
            feet_model: 0,
            weapon_dye: 0,
            body_dye: 0,
            hand_dye: 0,
            feet_dye: 0,
            underwear_dye: 0,
            style_back_dye: 0,
            style_head_dye: 0,
            style_face_dye: 0,
            weapon_enchant: 0,
            is_world_event_target: false,
            infamy: 0,
            show_face: true,
            style_head: 0,
            style_face: 0,
            style_back: 0,
            style_weapon: 0,
            style_body: 0,
            style_footprint: 0,
            style_body_dye: 0,
            show_style: true,
            title_count: 0,
            appearance2: user.appearance2,
            scale: 1.0,
            guild_logo_id: 0,
        },
    })
}

fn assemble_response_load_topo(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(ResponseLoadTopo {
        connection_global_world_id,
        packet: SLoadTopo {
            zone: 5,
            location: Vec3f {
                x: 16260.0,
                y: 1253.0,
                z: -4410.0,
            },
            disable_loading_screen: false,
        },
    })
}

fn assemble_response_load_hint(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(ResponseLoadHint {
        connection_global_world_id,
        packet: SLoadHint { unk1: 0 },
    })
}

fn assemble_user_ready_to_connect(connection_local_world_id: EntityId) -> EcsMessage {
    Box::new(UserReadyToConnect {
        connection_local_world_id,
    })
}

// TODO we somehow need to track the "is_alive" status between spawns
fn assemble_prepare_user_spawn(
    connection_global_world_id: EntityId,
    connection_channel: Sender<EcsMessage>,
    user: entity::User,
    location: entity::UserLocation,
) -> EcsMessage {
    Box::new(PrepareUserSpawn {
        user_initializer: UserInitializer {
            connection_global_world_id,
            connection_channel,
            user,
            location,
            is_alive: true,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::component::GlobalConnection;
    use crate::ecs::message::Message;
    use crate::model::entity::{Account, User, UserLocation};
    use crate::model::repository::{account, user};
    use crate::model::tests::db_test;
    use crate::model::{Class, Gender, PasswordHashAlgorithm, Race};
    use crate::protocol::serde::from_vec;
    use crate::Result;
    use async_std::sync::{channel, Receiver};
    use chrono::{TimeZone, Utc};
    use nalgebra::{Point3, Rotation3, Vector3};
    use sqlx::PgPool;
    use std::time::Instant;

    async fn setup(pool: PgPool) -> Result<(World, EntityId, Receiver<EcsMessage>, Account, User)> {
        let mut conn = pool.acquire().await?;

        let world = World::new();
        world.add_unique(pool);

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

        user_location::create(
            &mut conn,
            &UserLocation {
                user_id: user.id,
                zone: 0,
                point: Point3::new(1.0f32, 2.0f32, 3.0f32),
                rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 3.0),
            },
        )
        .await?;

        let (tx_channel, rx_channel) = channel(1024);

        let connection_global_world_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<GlobalConnection>| {
                entities.add_entity(
                    &mut connections,
                    GlobalConnection {
                        channel: tx_channel,
                        is_version_checked: false,
                        is_authenticated: false,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        Ok((world, connection_global_world_id, rx_channel, account, user))
    }

    async fn setup_with_connection(
        pool: PgPool,
    ) -> Result<(World, EntityId, Receiver<EcsMessage>)> {
        let world = World::new();
        world.add_unique(pool);

        let (tx_channel, rx_channel) = channel(1024);

        let connection_global_world_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<GlobalConnection>| {
                entities.add_entity(
                    &mut connections,
                    GlobalConnection {
                        channel: tx_channel,
                        is_version_checked: false,
                        is_authenticated: false,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        Ok((world, connection_global_world_id, rx_channel))
    }

    #[test]
    fn test_request_select_user() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel, account, user) =
                task::block_on(async { setup(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestSelectUser {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CSelectUser {
                                database_id: user.id,
                                unk1: 0,
                            },
                        }),
                    );
                },
            );

            world.run(user_spawner_system);

            let spawns = world.borrow::<View<GlobalUserSpawn>>();
            let spawn = spawns.get(connection_global_world_id);
            assert_eq!(spawn.account_id, account.id);
            assert_eq!(spawn.user_id, user.id);
            assert_eq!(spawn.status, UserSpawnStatus::Requesting);
            assert_eq!(spawn.marked_for_deletion, false);
            assert_eq!(spawn.is_alive, true);
            assert_eq!(spawn.local_world_id, None);
            assert_eq!(spawn.connection_local_world_id, None);

            Ok(())
        })
    }

    #[test]
    fn test_request_user_spawn_prepared() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, rx_channel, account, user) =
                task::block_on(async { setup(pool).await })?;

            // FIXME Ask upstream project to create a better way to create EntityIds
            let local_world_id =
                from_vec::<EntityId>(vec![0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
            let (local_world_tx, local_world_rx) = channel(100);

            world.run(
                |entities: EntitiesViewMut, mut spawns: ViewMut<GlobalUserSpawn>| {
                    entities.add_component(
                        &mut spawns,
                        GlobalUserSpawn {
                            connection_local_world_id: None,
                            user_id: user.id,
                            account_id: account.id,
                            status: UserSpawnStatus::Requesting,
                            zone_id: 0,
                            local_world_id: Some(local_world_id),
                            local_world_channel: Some(local_world_tx),
                            marked_for_deletion: false,
                            is_alive: true,
                        },
                        connection_global_world_id,
                    );
                },
            );

            let connection_local_world_id =
                from_vec::<EntityId>(vec![0x11, 0x00, 0x1D, 0x0, 0x0, 0x80, 0, 0])?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::UserSpawnPrepared {
                            connection_global_world_id,
                            connection_local_world_id,
                        }),
                    )
                },
            );

            world.run(user_spawner_system);

            let spawns = world.borrow::<View<GlobalUserSpawn>>();
            let spawn = spawns.get(connection_global_world_id);
            assert_eq!(spawn.status, UserSpawnStatus::Waiting);
            assert_eq!(
                spawn.connection_local_world_id,
                Some(connection_local_world_id)
            );

            match &*rx_channel.try_recv()? {
                Message::RegisterLocalWorld {
                    connection_local_world_id: id,
                    local_world_channel,
                } => {
                    assert_eq!(*id, connection_local_world_id);
                    assert!(!local_world_channel.is_full());
                }
                _ => panic!("Message is not a RegisterLocalWorld message"),
            }

            match &*rx_channel.try_recv()? {
                Message::ResponseLogin {
                    connection_global_world_id: id,
                    account_id,
                    user_id,
                    packet,
                } => {
                    assert_eq!(*id, connection_global_world_id);
                    assert_eq!(*user_id, user.id);
                    assert_eq!(*account_id, account.id);
                    assert_eq!(packet.id, connection_global_world_id);
                    assert!(packet.alive);
                }
                _ => panic!("Message is not a ResponseLogin message"),
            }

            match &*rx_channel.try_recv()? {
                Message::ResponseLoadTopo {
                    connection_global_world_id: id,
                    packet,
                } => {
                    assert_eq!(*id, connection_global_world_id);
                    assert_eq!(packet.disable_loading_screen, false);
                }
                _ => panic!("Message is not a ResponseLoadTopo message"),
            }

            match &*rx_channel.try_recv()? {
                Message::ResponseLoadHint {
                    connection_global_world_id: id,
                    packet,
                } => {
                    assert_eq!(*id, connection_global_world_id);
                    assert_eq!(packet.unk1, 0x0);
                }
                _ => panic!("Message is not a ResponseLoadHint message"),
            }

            match &*local_world_rx.try_recv()? {
                Message::UserReadyToConnect {
                    connection_local_world_id: id,
                } => {
                    assert_eq!(*id, connection_local_world_id);
                }
                _ => panic!("Message is not a UserReadyToConnect message"),
            }

            Ok(())
        })
    }

    #[test]
    fn test_user_spawned() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel, account, user) =
                task::block_on(async { setup(pool).await })?;

            world.run(
                |entities: EntitiesViewMut, mut spawns: ViewMut<GlobalUserSpawn>| {
                    entities.add_component(
                        &mut spawns,
                        GlobalUserSpawn {
                            connection_local_world_id: None,
                            user_id: user.id,
                            account_id: account.id,
                            status: UserSpawnStatus::Spawning,
                            zone_id: 0,
                            local_world_id: None,
                            local_world_channel: None,
                            marked_for_deletion: false,
                            is_alive: true,
                        },
                        connection_global_world_id,
                    );
                },
            );

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::UserSpawned {
                            connection_global_world_id,
                        }),
                    );
                },
            );

            world.run(user_spawner_system);

            let spawns = world.borrow::<View<GlobalUserSpawn>>();
            let spawn = spawns.get(connection_global_world_id);
            assert_eq!(spawn.account_id, account.id);
            assert_eq!(spawn.user_id, user.id);
            assert_eq!(spawn.status, UserSpawnStatus::Spawned);

            Ok(())
        })
    }

    #[test]
    fn test_prepare_local_spawn() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel, account, user) =
                task::block_on(async { setup(pool).await })?;

            // FIXME Ask upstream project to create a better way to create EntityIds
            let local_world_id =
                from_vec::<EntityId>(vec![0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
            let (local_world_tx, local_world_rx) = channel(100);

            world.run(
                |entities: EntitiesViewMut, mut spawns: ViewMut<GlobalUserSpawn>| {
                    entities.add_component(
                        &mut spawns,
                        GlobalUserSpawn {
                            connection_local_world_id: None,
                            user_id: user.id,
                            account_id: account.id,
                            status: UserSpawnStatus::CanSpawn,
                            zone_id: 0,
                            local_world_id: Some(local_world_id),
                            local_world_channel: Some(local_world_tx),
                            marked_for_deletion: false,
                            is_alive: true,
                        },
                        connection_global_world_id,
                    );
                },
            );

            world.run(user_spawner_system);

            match &*local_world_rx.try_recv()? {
                Message::PrepareUserSpawn { user_initializer } => {
                    assert_eq!(
                        user_initializer.connection_global_world_id,
                        connection_global_world_id
                    );
                    assert_eq!(user_initializer.user, user);
                }
                _ => panic!("Message is not a PrepareUserSpawn message"),
            }

            Ok(())
        })
    }

    #[test]
    #[should_panic(expected = "SPAWN FAILED")]
    fn test_user_spawn_failed() {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, _rx_channel) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |entities: EntitiesViewMut, mut spawns: ViewMut<GlobalUserSpawn>| {
                    entities.add_component(
                        &mut spawns,
                        GlobalUserSpawn {
                            connection_local_world_id: None,
                            user_id: 0,
                            account_id: 0,
                            status: UserSpawnStatus::SpawnFailed,
                            zone_id: 0,
                            local_world_id: None,
                            local_world_channel: None,
                            marked_for_deletion: false,
                            is_alive: true,
                        },
                        connection_global_world_id,
                    );
                },
            );

            world.run(user_spawner_system);

            Ok(())
        })
        .unwrap();
    }
}
