use crate::ecs::component::{LocalConnection, LocalUserSpawn, Location, UserSpawnStatus};
use crate::ecs::dto::{UserFinalizer, UserInitializer};
use crate::ecs::message::Message::{
    ResponseSpawnMe, UserDespawned, UserSpawnPrepared, UserSpawned,
};
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::resource::{DeletionList, GlobalMessageChannel};
use crate::ecs::system::send_message;
use crate::model::entity::UserLocation;
use crate::model::{Angle, Vec3f};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::{ensure, Context};
use shipyard::*;
use tracing::{debug, error, info_span};

/// Acts as a gateway for users to pass when spawning / logging out.
pub fn user_gateway_system(
    incoming_messages: View<EcsMessage>,
    mut connections: ViewMut<LocalConnection>,
    mut user_spawns: ViewMut<LocalUserSpawn>,
    mut locations: ViewMut<Location>,
    mut entities: EntitiesViewMut,
    global_world_channel: UniqueView<GlobalMessageChannel>,
    mut deletion_list: UniqueViewMut<DeletionList>,
) {
    (&incoming_messages)
        .iter()
        .for_each(|message| match &**message {
            Message::PrepareUserSpawn { user_initializer } => {
                let connection_global_world_id = user_initializer.connection_global_world_id;
                id_span!(connection_global_world_id);
                handle_prepare_user_spawn(
                    &user_initializer,
                    &mut connections,
                    &mut user_spawns,
                    &mut locations,
                    &mut entities,
                    &global_world_channel,
                )
            }
            Message::UserReadyToConnect {
                connection_local_world_id,
            } => {
                id_span!(connection_local_world_id);
                if let Err(e) =
                    handle_user_ready_to_connect(*connection_local_world_id, &mut user_spawns)
                {
                    // TODO Somehow cleanup LocalConnections that didn't connect in time
                    error!("Ignoring Message::UserReadyToConnect: {:?}", e);
                }
            }
            Message::RequestLoadTopoFin {
                connection_global_world_id,
                connection_local_world_id,
                ..
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_load_topo_fin(
                    *connection_global_world_id,
                    *connection_local_world_id,
                    &mut connections,
                    &mut user_spawns,
                    &mut locations,
                    &global_world_channel,
                ) {
                    // TODO Somehow cleanup LocalConnections that didn't connect in time
                    error!("Ignoring Message::RequestLoadTopoFin: {:?}", e);
                }
            }
            Message::UserDespawn {
                connection_local_world_id,
            } => {
                id_span!(connection_local_world_id);
                if let Err(e) = handle_user_despawn(
                    *connection_local_world_id,
                    &mut user_spawns,
                    &mut locations,
                    &mut deletion_list,
                    &global_world_channel,
                ) {
                    error!("Ignoring Message::UserDespawn: {:?}", e);
                }
            }
            _ => { /* Ignore all other messages */ }
        });
}

fn handle_prepare_user_spawn(
    user_initializer: &UserInitializer,
    connections: &mut ViewMut<LocalConnection>,
    user_spawns: &mut ViewMut<LocalUserSpawn>,
    locations: &mut ViewMut<Location>,
    entities: &mut EntitiesViewMut,
    global_world_channel: &UniqueView<GlobalMessageChannel>,
) {
    debug!("Message::PrepareUserSpawn incoming");

    let connection_local_world_id = entities.add_entity(
        (connections, user_spawns, locations),
        (
            LocalConnection {
                channel: user_initializer.connection_channel.clone(),
            },
            LocalUserSpawn {
                connection_global_world_id: user_initializer.connection_global_world_id,
                user_id: user_initializer.user.id,
                account_id: user_initializer.user.account_id,
                status: UserSpawnStatus::Waiting,
                zone_id: user_initializer.location.zone_id,
                is_alive: user_initializer.is_alive,
            },
            Location {
                point: user_initializer.location.point.clone(),
                rotation: user_initializer.location.rotation.clone(),
            },
        ),
    );

    send_message(
        assemble_user_spawn_prepared(
            user_initializer.connection_global_world_id,
            connection_local_world_id,
        ),
        &global_world_channel.channel,
    );
}

fn handle_user_ready_to_connect(
    connection_local_world_id: EntityId,
    user_spawns: &mut ViewMut<LocalUserSpawn>,
) -> Result<()> {
    debug!("Message::UserReadyToConnect incoming");

    let mut spawn = user_spawns
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't get local user spawn {:?}",
            connection_local_world_id
        ))?;
    spawn.status = UserSpawnStatus::CanSpawn;

    Ok(())
}

fn handle_load_topo_fin(
    connection_global_world_id: EntityId,
    connection_local_world_id: EntityId,
    connections: &mut ViewMut<LocalConnection>,
    user_spawns: &mut ViewMut<LocalUserSpawn>,
    locations: &mut ViewMut<Location>,
    global_world_channel: &UniqueView<GlobalMessageChannel>,
) -> Result<()> {
    debug!("Message::RequestLoadTopoFin incoming");

    let (connection, spawn, location) = (connections, user_spawns, locations)
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't find connection with local spawn for {:?}",
            connection_local_world_id
        ))?;

    ensure!(
        spawn.status == UserSpawnStatus::CanSpawn,
        "User sends Message::RequestLoadTopoFin too early"
    );

    // Spawn the user and tell the global world that the user is spawned
    send_message(
        assemble_response_spawn_me(
            connection_global_world_id,
            connection_local_world_id,
            location,
            spawn.is_alive,
        ),
        &connection.channel,
    );
    send_message(
        assemble_user_spawned(connection_global_world_id),
        &global_world_channel.channel,
    );

    spawn.status = UserSpawnStatus::Spawned;

    Ok(())
}

fn handle_user_despawn(
    connection_local_world_id: EntityId,
    user_spawns: &mut ViewMut<LocalUserSpawn>,
    locations: &mut ViewMut<Location>,
    deletion_list: &mut UniqueViewMut<DeletionList>,
    global_world_channel: &UniqueView<GlobalMessageChannel>,
) -> Result<()> {
    debug!("Message::UserDespawn incoming");

    let (spawn, location) = (user_spawns, locations)
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't find local spawn for {:?}",
            connection_local_world_id
        ))?;

    // Send all user data that needs to be persisted to the global world.
    send_message(
        assemble_user_despawned(spawn, location),
        &global_world_channel.channel,
    );

    deletion_list.0.push(connection_local_world_id);
    debug!(
        "Marked local user {:?} for deletion",
        connection_local_world_id
    );
    Ok(())
}

fn assemble_response_spawn_me(
    connection_global_world_id: EntityId,
    connection_local_world_id: EntityId,
    location: &Location,
    is_alive: bool,
) -> EcsMessage {
    Box::new(ResponseSpawnMe {
        connection_global_world_id,
        connection_local_world_id,
        packet: SSpawnMe {
            user_id: connection_local_world_id,
            location: Vec3f {
                x: location.point.x,
                y: location.point.y,
                z: location.point.z,
            },
            rotation: Angle::from(location.rotation.clone()),
            is_alive,
            is_lord: false,
        },
    })
}

fn assemble_user_spawned(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(UserSpawned {
        connection_global_world_id,
    })
}

fn assemble_user_despawned(spawn: &LocalUserSpawn, location: &Location) -> EcsMessage {
    Box::new(UserDespawned {
        user_finalizer: UserFinalizer {
            connection_global_world_id: spawn.connection_global_world_id,
            user_id: spawn.user_id,
            location: UserLocation {
                user_id: spawn.user_id,
                zone_id: spawn.zone_id,
                point: location.point.clone(),
                rotation: location.rotation.clone(),
            },
            is_alive: spawn.is_alive,
        },
    })
}

fn assemble_user_spawn_prepared(
    connection_global_world_id: EntityId,
    connection_local_world_id: EntityId,
) -> EcsMessage {
    Box::new(UserSpawnPrepared {
        connection_global_world_id,
        connection_local_world_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::entity::{User, UserLocation};
    use crate::model::{Class, Gender, Race};
    use crate::protocol::serde::from_vec;
    use crate::Result;
    use async_std::sync::{channel, Receiver};
    use chrono::{TimeZone, Utc};
    use nalgebra::{Point3, Rotation3, Vector3};

    fn setup() -> Result<(World, Receiver<EcsMessage>)> {
        let (global_tx_channel, global_rx_channel) = channel(1024);

        let world = World::new();
        world.add_unique(GlobalMessageChannel {
            channel: global_tx_channel.clone(),
        });

        world.add_unique(DeletionList(Vec::default()));

        Ok((world, global_rx_channel))
    }

    fn setup_with_spawn() -> Result<(World, EntityId, Receiver<EcsMessage>, Receiver<EcsMessage>)> {
        let (world, global_rx_channel) = setup()?;

        let (connection_tx_channel, connection_rx_channel) = channel(1024);

        let connection_local_world_id = world.run(
            |mut entities: EntitiesViewMut,
             mut connections: ViewMut<LocalConnection>,
             mut user_spawns: ViewMut<LocalUserSpawn>,
             mut locations: ViewMut<Location>| {
                entities.add_entity(
                    (&mut connections, &mut user_spawns, &mut locations),
                    (
                        LocalConnection {
                            channel: connection_tx_channel,
                        },
                        LocalUserSpawn {
                            user_id: 1,
                            account_id: 1,
                            status: UserSpawnStatus::Waiting,
                            zone_id: 0,
                            connection_global_world_id: from_vec::<EntityId>(vec![
                                0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                            ])
                            .unwrap(),
                            is_alive: true,
                        },
                        Location {
                            point: Point3::new(2.0f32, 3.0f32, 3.0f32),
                            rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 1.0),
                        },
                    ),
                )
            },
        );

        Ok((
            world,
            connection_local_world_id,
            global_rx_channel,
            connection_rx_channel,
        ))
    }

    #[test]
    fn test_prepare_user_spawn() -> Result<()> {
        let (world, global_rx_channel) = setup()?;

        let connection_global_world_id =
            from_vec::<EntityId>(vec![0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
        let (connection_tx, _connection_rx) = channel(100);

        let user = User {
            id: 1,
            account_id: 1,
            name: "TestUser".to_string(),
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
            lobby_slot: 0,
            is_new_character: false,
            tutorial_state: 0,
            is_deleting: false,
            delete_at: None,
            last_logout_at: Utc.ymd(2020, 7, 8).and_hms(9, 10, 11),
            created_at: Utc.ymd(2020, 7, 8).and_hms(9, 10, 11),
        };

        let user_location = UserLocation {
            user_id: 1,
            zone_id: 0,
            point: Point3::new(1.0, 1.0, 1.0),
            rotation: Rotation3::from_axis_angle(&Vector3::z_axis(), 0.0),
        };

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::PrepareUserSpawn {
                        user_initializer: UserInitializer {
                            connection_global_world_id,
                            connection_channel: connection_tx,
                            user: user.clone(),
                            location: user_location.clone(),
                            is_alive: true,
                        },
                    }),
                );
            },
        );

        world.run(user_gateway_system);

        let connection_local_world_id = world.run(
            |connections: View<LocalConnection>,
             spawns: View<LocalUserSpawn>,
             locations: View<Location>| {
                let (id, (_connection, spawn, location)) = (&connections, &spawns, &locations)
                    .iter()
                    .with_id()
                    .next()
                    .unwrap();
                assert_eq!(spawn.connection_global_world_id, connection_global_world_id);
                assert_eq!(spawn.user_id, user.id);
                assert_eq!(spawn.account_id, user.account_id);
                assert_eq!(spawn.status, UserSpawnStatus::Waiting);
                assert_eq!(spawn.zone_id, 0);
                assert_eq!(spawn.is_alive, true);
                assert_eq!(location.point, user_location.point);
                assert_eq!(location.rotation, user_location.rotation);

                Ok::<EntityId, anyhow::Error>(id)
            },
        )?;

        match &*global_rx_channel.try_recv()? {
            Message::UserSpawnPrepared {
                connection_global_world_id: gid,
                connection_local_world_id: lid,
            } => {
                assert_eq!(*gid, connection_global_world_id);
                assert_eq!(*lid, connection_local_world_id);
            }
            _ => panic!("Can't find Message::UserSpawnPrepared"),
        }

        Ok(())
    }

    #[test]
    fn test_user_ready_to_connect() -> Result<()> {
        let (world, connection_local_world_id, _global_rx_channel, _connection_rx_channel) =
            setup_with_spawn()?;

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::UserReadyToConnect {
                        connection_local_world_id,
                    }),
                );
            },
        );

        world.run(user_gateway_system);

        world.run(|spawns: View<LocalUserSpawn>| {
            let spawn = spawns.try_get(connection_local_world_id)?;
            assert_eq!(spawn.status, UserSpawnStatus::CanSpawn);

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    // Fix the test
    #[test]
    fn test_load_topo_fin() -> Result<()> {
        let (world, connection_local_world_id, global_rx_channel, connection_rx_channel) =
            setup_with_spawn()?;

        world.run(|mut spawns: ViewMut<LocalUserSpawn>| {
            let mut spawn = (&mut spawns).try_get(connection_local_world_id)?;
            spawn.status = UserSpawnStatus::CanSpawn;

            Ok::<(), anyhow::Error>(())
        })?;

        let connection_global_world_id =
            from_vec::<EntityId>(vec![0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::RequestLoadTopoFin {
                        connection_global_world_id,
                        connection_local_world_id,
                        packet: CLoadTopoFin {},
                    }),
                );
            },
        );

        world.run(user_gateway_system);

        world.run(|spawns: View<LocalUserSpawn>, locations: View<Location>| {
            // User entity needs to have both a LocalUserSpawn and a Location component attached
            let (spawn, location) = (&spawns, &locations).try_get(connection_local_world_id)?;
            assert_eq!(spawn.status, UserSpawnStatus::Spawned);

            match &*connection_rx_channel.try_recv()? {
                Message::ResponseSpawnMe {
                    connection_global_world_id: gid,
                    connection_local_world_id: lid,
                    packet,
                } => {
                    assert_eq!(*gid, connection_global_world_id);
                    assert_eq!(*lid, connection_local_world_id);
                    assert_eq!(packet.user_id, connection_local_world_id);
                    assert_eq!(packet.location.x, location.point.x);
                    assert_eq!(packet.location.y, location.point.y);
                    assert_eq!(packet.location.z, location.point.z);
                }
                _ => panic!("Can't find Message::ResponseSpawnMe"),
            }

            match &*global_rx_channel.try_recv()? {
                Message::UserSpawned {
                    connection_global_world_id: gid,
                } => {
                    assert_eq!(*gid, connection_global_world_id);
                }
                _ => panic!("Can't find Message::UserSpawned"),
            }

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    #[test]
    fn test_load_topo_fin_too_early() -> Result<()> {
        let (world, connection_local_world_id, _global_rx_channel, _connection_rx_channel) =
            setup_with_spawn()?;

        world.run(|mut spawns: ViewMut<LocalUserSpawn>| {
            let mut spawn = (&mut spawns).try_get(connection_local_world_id)?;
            spawn.status = UserSpawnStatus::Waiting;

            Ok::<(), anyhow::Error>(())
        })?;

        let connection_global_world_id =
            from_vec::<EntityId>(vec![0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::RequestLoadTopoFin {
                        connection_global_world_id,
                        connection_local_world_id,
                        packet: CLoadTopoFin {},
                    }),
                );
            },
        );

        world.run(user_gateway_system);

        world.run(|spawns: View<LocalUserSpawn>| {
            let spawn = spawns.try_get(connection_local_world_id)?;
            assert_eq!(spawn.status, UserSpawnStatus::Waiting);

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    #[test]
    fn test_user_despawn() -> Result<()> {
        let (world, connection_local_world_id, global_rx_channel, _connection_rx_channel) =
            setup_with_spawn()?;

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::UserDespawn {
                        connection_local_world_id,
                    }),
                );
            },
        );

        world.run(user_gateway_system);

        world.run(|mut deletion_list: UniqueViewMut<DeletionList>| {
            assert_eq!(deletion_list.0.len(), 1);
            assert_eq!(deletion_list.0.pop(), Some(connection_local_world_id));

            Ok::<(), anyhow::Error>(())
        })?;

        world.run(|spawns: View<LocalUserSpawn>, locations: View<Location>| {
            let (spawn, location) = (&spawns, &locations).try_get(connection_local_world_id)?;

            match &*global_rx_channel.try_recv()? {
                Message::UserDespawned { user_finalizer } => {
                    assert_eq!(
                        user_finalizer.connection_global_world_id,
                        spawn.connection_global_world_id
                    );
                    assert_eq!(user_finalizer.user_id, spawn.user_id);
                    assert_eq!(user_finalizer.location.point, location.point);
                    assert_eq!(user_finalizer.location.rotation, location.rotation);
                    assert_eq!(user_finalizer.is_alive, spawn.is_alive);
                }
                _ => panic!("Can't find Message::UserDespawned"),
            }

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }
}
