use crate::ecs::component::{LocalConnection, LocalUserSpawn, UserSpawnStatus};
use crate::ecs::dto::UserInitializer;
use crate::ecs::message::Message::{ResponseSpawnMe, UserSpawnPrepared, UserSpawned};
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::resource::{DeletionList, GlobalMessageChannel};
use crate::ecs::system::send_message;
use crate::model::{Angle, Vec3};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::{ensure, Context};
use async_std::task;
use shipyard::*;
use tracing::{debug, error, info_span};

// TODO write tests for the user_spawner_system

/// Acts as a gateway for users to pass when spawning / logging out.
pub fn user_gateway_system(
    incoming_messages: View<EcsMessage>,
    mut connections: ViewMut<LocalConnection>,
    mut user_spawns: ViewMut<LocalUserSpawn>,
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
                    // TODO decide what to do in an error case
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
                    &global_world_channel,
                ) {
                    // TODO decide what to do in an error case
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
                    &mut deletion_list,
                ) {
                    // TODO decide what to do in an error case
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
    entities: &mut EntitiesViewMut,
    global_world_channel: &UniqueView<GlobalMessageChannel>,
) {
    debug!("Message::PrepareUserSpawn incoming");

    let connection_local_world_id = entities.add_entity(
        user_spawns,
        LocalUserSpawn {
            user_id: user_initializer.user.id,
            account_id: user_initializer.user.account_id,
            status: UserSpawnStatus::Waiting,
            is_alive: true,
        },
    );

    entities.add_component(
        connections,
        LocalConnection {
            channel: user_initializer.connection_channel.clone(),
        },
        connection_local_world_id,
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
    global_world_channel: &UniqueView<GlobalMessageChannel>,
) -> Result<()> {
    debug!("Message::RequestLoadTopoFin incoming");

    let connection = connections
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't find connection {:?}",
            connection_local_world_id
        ))?;

    let spawn = user_spawns
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't find local user spawn {:?}",
            connection_local_world_id
        ))?;

    ensure!(
        spawn.status == UserSpawnStatus::CanSpawn,
        "User sends Message::RequestLoadTopoFin too early"
    );

    Ok(task::block_on(async {
        // Spawn the user and the global world that the user is spawned
        // TODO use the coordinates in the LocalUserSpawn component
        send_message(
            assemble_response_spawn_me(connection_global_world_id, connection_local_world_id),
            &connection.channel,
        );
        send_message(
            assemble_user_spawned(connection_global_world_id),
            &global_world_channel.channel,
        );

        spawn.status = UserSpawnStatus::Spawned;

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_user_despawn(
    connection_local_world_id: EntityId,
    user_spawns: &mut ViewMut<LocalUserSpawn>,
    deletion_list: &mut UniqueViewMut<DeletionList>,
) -> Result<()> {
    debug!("Message::UserDespawn incoming");

    let spawn = user_spawns
        .try_get(connection_local_world_id)
        .context(format!(
            "Can't get local user spawn {:?}",
            connection_local_world_id
        ))?;

    ensure!(
        spawn.status == UserSpawnStatus::Spawned,
        "Can't de-spawn a user that's isn't spawned yet"
    );

    // TODO we need to send the global world the data that we hold and need persistence (like exp, playtime etc.)

    deletion_list.0.push(connection_local_world_id);
    debug!("Marked local user entity for deletion");
    Ok(())
}

fn assemble_response_spawn_me(
    connection_global_world_id: EntityId,
    connection_local_world_id: EntityId,
) -> EcsMessage {
    Box::new(ResponseSpawnMe {
        connection_global_world_id,
        connection_local_world_id,
        packet: SSpawnMe {
            user_id: connection_local_world_id,
            location: Vec3 {
                x: 16260.0,
                y: 1253.0,
                z: -4410.0,
            },
            rotation: Angle::from_deg(342.0),
            is_alive: true,
            is_lord: false,
        },
    })
}

fn assemble_user_spawned(connection_global_world_id: EntityId) -> EcsMessage {
    Box::new(UserSpawned {
        connection_global_world_id,
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
