use crate::ecs::component::GlobalConnection;
use crate::ecs::message::Message::ResponseGetUserList;
use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::system::global::send_message_to_connection;
use crate::model::entity::User;
use crate::model::repository::user;
use crate::model::{Vec3, Vec3a};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::{ensure, Context};
use async_std::task;
use chrono::Utc;
use lazy_static::lazy_static;
use regex::Regex;
use shipyard::*;
use sqlx::{PgConnection, PgPool};
use std::cmp::min;
use tracing::{debug, error, info, info_span};

const MAX_USERS_PER_ACCOUNT: usize = 20;
const CHUNK_SIZE: usize = 5;

/// Handles the users of an account. Users in TERA terminology are the player characters of an account.
pub fn user_manager_system(
    incoming_messages: View<EcsMessage>,
    connections: View<GlobalConnection>,
    pool: UniqueView<PgPool>,
) {
    // TODO Look for users without a connection component. Set their "deletion time" and persist them ones reached.
    (&incoming_messages)
        .iter()
        .for_each(|message| match &**message {
            Message::RequestCanCreateUser {
                connection_global_world_id,
                account_id,
                ..
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_can_create_user(
                    *connection_global_world_id,
                    *account_id,
                    &connections,
                    &pool,
                ) {
                    error!("Rejecting create user request: {:?}", e);
                    send_message_to_connection(
                        assemble_can_create_user_response(*connection_global_world_id, false),
                        &connections,
                    );
                }
            }
            Message::RequestChangeUserLobbySlotId {
                connection_global_world_id,
                account_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_change_user_lobby_slot_id(&packet, *account_id, &pool) {
                    error!("Ignoring change user lobby slot id request: {:?}", e);
                }
            }
            Message::RequestGetUserList {
                connection_global_world_id,
                account_id,
                ..
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_user_list(
                    *connection_global_world_id,
                    *account_id,
                    &connections,
                    &pool,
                ) {
                    error!("Rejecting get user list request: {:?}", e);
                    send_message_to_connection(
                        assemble_user_list_response(
                            *connection_global_world_id,
                            &Vec::new(),
                            true,
                            true,
                        ),
                        &connections,
                    );
                }
            }
            Message::RequestCheckUserName {
                connection_global_world_id,
                packet,
                ..
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_check_user_name(
                    &packet,
                    *connection_global_world_id,
                    &connections,
                    &pool,
                ) {
                    error!("Rejecting check user name request: {:?}", e);
                    send_message_to_connection(
                        assemble_check_user_name_response(*connection_global_world_id, false),
                        &connections,
                    );
                }
            }
            Message::RequestCreateUser {
                connection_global_world_id,
                account_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_create_user(
                    &packet,
                    *connection_global_world_id,
                    *account_id,
                    &connections,
                    &pool,
                ) {
                    error!("Rejecting create user request: {:?}", e);
                    send_message_to_connection(
                        assemble_create_user_response(*connection_global_world_id, false),
                        &connections,
                    );
                }
            }
            Message::RequestDeleteUser {
                connection_global_world_id,
                account_id,
                packet,
            } => {
                id_span!(connection_global_world_id);
                if let Err(e) = handle_delete_user(
                    &packet,
                    *connection_global_world_id,
                    *account_id,
                    &connections,
                    &pool,
                ) {
                    error!("Rejecting delete user request: {:?}", e);
                    send_message_to_connection(
                        assemble_delete_user_response(*connection_global_world_id, false),
                        &connections,
                    );
                }
            }
            _ => { /* Ignore all other messages */ }
        });
}

fn handle_user_list(
    connection_global_world_id: EntityId,
    account_id: i64,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Get user list message incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        // Send the user list paged, since we can only send 16kiB of data in one packet
        let mut is_first_page = true;

        let users = user::list(&mut conn, account_id).await?;

        if users.len() == 0 {
            send_message_to_connection(
                assemble_user_list_response(connection_global_world_id, &Vec::new(), true, true),
                connections,
            );
        } else {
            let chunk_count = users.chunks(CHUNK_SIZE).count();
            let mut current_chunk = 1;

            for chunk in users.chunks(CHUNK_SIZE) {
                let is_last_page = if current_chunk == chunk_count {
                    true
                } else {
                    false
                };

                send_message_to_connection(
                    assemble_user_list_response(
                        connection_global_world_id,
                        chunk,
                        is_first_page,
                        is_last_page,
                    ),
                    connections,
                );

                is_first_page = false;
                current_chunk += 1;
            }
        }

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_can_create_user(
    connection_global_world_id: EntityId,
    account_id: i64,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestCanCreateUser incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if can_create_user(&mut conn, account_id).await? {
            send_message_to_connection(
                assemble_can_create_user_response(connection_global_world_id, true),
                connections,
            );
        } else {
            send_message_to_connection(
                assemble_can_create_user_response(connection_global_world_id, false),
                connections,
            );
        }

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_change_user_lobby_slot_id(
    packet: &CChangeUserLobbySlotId,
    account_id: i64,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestChangeUserLobbySlotId incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .begin()
            .await
            .context("Couldn't acquire connection from pool")?;

        let mut user_list = packet.user_positions.clone();
        user_list.sort_by(|a, b| a.lobby_slot.partial_cmp(&b.lobby_slot).unwrap());

        for (pos, entry) in user_list.iter().enumerate() {
            let db_user = user::get_by_id(&mut conn, entry.database_id)
                .await
                .context(format!("Can't find user {}", entry.database_id))?;
            ensure!(
                db_user.account_id == account_id,
                "User {} doesn't belong to account {}",
                entry.database_id,
                account_id
            );
            // Client starts the lobby slot at 1
            debug!(
                "Updating lobby slot of user id {} to {}",
                db_user.id,
                pos + 1
            );
            user::update_lobby_slot(&mut conn, entry.database_id, (pos + 1) as i32)
                .await
                .context("Can't update the lobby slot of  user")?;
        }

        conn.commit().await?;

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_create_user(
    packet: &CCreateUser,
    connection_global_world_id: EntityId,
    account_id: i64,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestCreateUser incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .begin()
            .await
            .context("Couldn't acquire connection from pool")?;

        // TODO validate the character even more

        if can_create_user(&mut conn, account_id).await?
            && check_username(&mut conn, &packet.name).await?
        {
            // Client starts the position at 1
            let next_position = 1 + user::get_user_count(&mut conn, account_id).await?;
            create_new_user(&mut conn, account_id, next_position as i32, packet).await?;
            send_message_to_connection(
                assemble_create_user_response(connection_global_world_id, true),
                connections,
            );
        } else {
            send_message_to_connection(
                assemble_create_user_response(connection_global_world_id, false),
                connections,
            );
        }

        conn.commit().await?;

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_delete_user(
    packet: &CDeleteUser,
    connection_global_world_id: EntityId,
    account_id: i64,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestDeleteUser incoming");

    // TODO if a global world_location component is attached to the connection, don't execute the command!
    // TODO Implement the deletion timer functionality

    Ok(task::block_on(async {
        let mut conn = pool
            .begin()
            .await
            .context("Couldn't acquire connection from pool")?;

        ensure!(
            user::get_by_id(&mut conn, packet.database_id).await.is_ok(),
            format!("Can't find user ID {} in the database", packet.database_id)
        );

        let db_user = user::get_by_id(&mut conn, packet.database_id)
            .await
            .context("Can't query user")?;
        ensure!(
            db_user.account_id == account_id,
            "User {} doesn't belong to account {}",
            db_user.id,
            account_id
        );

        user::delete_by_id(&mut conn, db_user.id)
            .await
            .context("Can't delete user")?;
        info!("Deleted user with ID {}", db_user.id);

        let users = user::list(&mut conn, account_id).await?;
        for (pos, user) in users.iter().enumerate() {
            if user.lobby_slot != pos as i32 {
                // Client starts the lobby slot at 1
                debug!("Updating lobby slot of user id {} to {}", user.id, pos + 1);
                user::update_lobby_slot(&mut conn, user.id, (pos + 1) as i32)
                    .await
                    .context("Can't update the lobby slot of user")?;
            }
        }

        send_message_to_connection(
            assemble_delete_user_response(connection_global_world_id, true),
            connections,
        );

        conn.commit().await?;

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_check_user_name(
    packet: &CCheckUserName,
    connection_global_world_id: EntityId,
    connections: &View<GlobalConnection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    debug!("Message::RequestCheckUserName incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if check_username(&mut conn, &packet.name).await? {
            send_message_to_connection(
                assemble_check_user_name_response(connection_global_world_id, true),
                connections,
            );
        } else {
            send_message_to_connection(
                assemble_check_user_name_response(connection_global_world_id, false),
                connections,
            );
        }

        Ok::<(), anyhow::Error>(())
    })?)
}

// Returns true if the name is valid and is not taken.
async fn check_username(mut conn: &mut PgConnection, name: &str) -> Result<bool> {
    if !is_valid_user_name(name) {
        info!("Invalid username provided");
        return Ok(false);
    }

    if user::is_user_name_taken(&mut conn, name).await? {
        Ok(false)
    } else {
        Ok(true)
    }
}

// Returns true if the account has free character slots.
async fn can_create_user(mut conn: &mut PgConnection, account_id: i64) -> Result<bool> {
    if MAX_USERS_PER_ACCOUNT as i64 > user::get_user_count(&mut conn, account_id).await? {
        Ok(true)
    } else {
        Ok(false)
    }
}

// Creates a new user with default values
async fn create_new_user(
    mut conn: &mut PgConnection,
    account_id: i64,
    lobby_slot: i32,
    packet: &CCreateUser,
) -> Result<()> {
    // TODO also create the default user_location
    user::create(
        &mut conn,
        &User {
            id: -1,
            account_id,
            name: packet.name.clone(),
            gender: packet.gender,
            race: packet.race,
            class: packet.class,
            shape: packet.shape.clone(),
            details: packet.details.clone(),
            appearance: packet.appearance.clone(),
            appearance2: packet.appearance2,
            level: 1,
            awakening_level: 0,
            laurel: -1,
            achievement_points: 0,
            playtime: 0,
            rest_bonus_xp: 419,
            show_face: false,
            show_style: false,
            lobby_slot,
            is_new_character: true,
            tutorial_state: 0,
            is_deleting: false,
            delete_at: None,
            last_logout_at: Utc::now(),
            created_at: Utc::now(),
        },
    )
    .await
    .context("Can't create user")?;
    Ok(())
}

/// Only alphanumeric characters are currently allowed. The client in rather limited with it's font.
fn is_valid_user_name(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"^[[:alnum:]]+$"#).unwrap();
    }
    RE.is_match(text)
}

fn assemble_can_create_user_response(connection_global_world_id: EntityId, ok: bool) -> EcsMessage {
    Box::new(Message::ResponseCanCreateUser {
        connection_global_world_id,
        packet: SCanCreateUser { ok },
    })
}

fn assemble_create_user_response(connection_global_world_id: EntityId, ok: bool) -> EcsMessage {
    Box::new(Message::ResponseCreateUser {
        connection_global_world_id,
        packet: SCreateUser { ok },
    })
}

fn assemble_check_user_name_response(connection_global_world_id: EntityId, ok: bool) -> EcsMessage {
    Box::new(Message::ResponseCheckUserName {
        connection_global_world_id,
        packet: SCheckUserName { ok },
    })
}

fn assemble_delete_user_response(connection_global_world_id: EntityId, ok: bool) -> EcsMessage {
    Box::new(Message::ResponseDeleteUser {
        connection_global_world_id,
        packet: SDeleteUser { ok },
    })
}

fn assemble_user_list_response(
    connection_global_world_id: EntityId,
    users: &[User],
    is_first_page: bool,
    is_last_page: bool,
) -> EcsMessage {
    // TODO calculate hp/mp/max_rest_bonus/world_id/guard_id/section_id and also return the equip / styles / custom strings / guild / has_broker_sales from db
    let characters = users
        .into_iter()
        .cloned()
        .map(move |user| {
            let delete_time = match user.delete_at {
                Some(t) => t.timestamp(),
                None => 0,
            };

            // FIXME Something is wrong with the custom_strings field! It needs to be set with zero values?!
            // FIXME test the deletion time stamps!
            SGetUserListCharacter {
                custom_strings: vec![SGetUserListCharacterCustomString {
                    string: "".to_string(),
                    id: 0,
                }],
                name: user.name,
                details: user.details,
                shape: user.shape,
                guild_name: "".to_string(),
                db_id: user.id,
                gender: user.gender,
                race: user.race,
                class: user.class,
                level: user.level,
                hp: 200,
                mp: 100,
                world_id: 0,
                guard_id: 0,
                section_id: 0,
                last_logout_time: user.last_logout_at.timestamp(),
                is_deleting: user.is_deleting,
                delete_time: 86400,
                delete_remain_sec: min(delete_time - Utc::now().timestamp(), -1_585_902_611) as i32,
                weapon: 0,
                earring1: 0,
                earring2: 0,
                body: 0,
                hand: 0,
                feet: 0,
                unk_item7: 0,
                ring1: 0,
                ring2: 0,
                underwear: 0,
                head: 0,
                face: 0,
                appearance: user.appearance,
                is_second_character: false,
                admin_level: 0,
                is_banned: false,
                ban_end_time: 0,
                ban_remain_sec: -1_585_989_011,
                rename_needed: 0,
                weapon_model: 0,
                unk_model2: 0,
                unk_model3: 0,
                body_model: 0,
                hand_model: 0,
                feet_model: 0,
                unk_model7: 0,
                unk_model8: 0,
                unk_model9: 0,
                unk_model10: 0,
                unk_dye1: 0,
                unk_dye2: 0,
                weapon_dye: 0,
                body_dye: 0,
                hand_dye: 0,
                feet_dye: 0,
                unk_dye7: 0,
                unk_dye8: 0,
                unk_dye9: 0,
                underwear_dye: 0,
                style_back_dye: 0,
                style_head_dye: 0,
                style_face_dye: 0,
                style_head: 0,
                style_face: 0,
                style_back: 0,
                style_weapon: 0,
                style_body: 0,
                style_footprint: 0,
                style_body_dye: 0,
                weapon_enchant: 0,
                rest_bonus_xp: user.rest_bonus_xp,
                max_rest_bonus_xp: 1,
                show_face: user.show_face,
                style_head_scale: 1.0,
                style_head_rotation: Vec3a::default(),
                style_head_translation: Vec3::default(),
                style_head_translation_debug: Vec3::default(),
                style_faces_scale: 1.0,
                style_face_rotation: Vec3a::default(),
                style_face_translation: Vec3::default(),
                style_face_translation_debug: Vec3::default(),
                style_back_scale: 1.0,
                style_back_rotation: Vec3a::default(),
                style_back_translation: Vec3::default(),
                style_back_translation_debug: Vec3::default(),
                used_style_head_transform: false,
                is_new_character: user.is_new_character,
                tutorial_state: user.tutorial_state,
                show_style: user.show_style,
                appearance2: user.appearance2,
                achievement_points: user.achievement_points,
                laurel: user.laurel,
                lobby_slot: user.lobby_slot,
                guild_logo_id: 0,
                awakening_level: user.awakening_level,
                has_broker_sales: false,
            }
        })
        .collect();

    Box::new(ResponseGetUserList {
        connection_global_world_id,
        packet: SGetUserList {
            characters,
            veteran: false,
            bonus_buf_sec: 0,
            max_characters: MAX_USERS_PER_ACCOUNT as i32,
            first: is_first_page,
            more: !is_last_page,
            left_del_time_account_over: 0,
            deletion_section_classify_level: 40,
            delete_character_expire_hour1: 0,
            delete_character_expire_hour2: 24,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::component::GlobalConnection;
    use crate::ecs::message::Message;
    use crate::model::entity::Account;
    use crate::model::repository::account;
    use crate::model::tests::db_test;
    use crate::model::{Class, Customization, Gender, PasswordHashAlgorithm, Race};
    use crate::Result;
    use async_std::sync::{channel, Receiver};
    use chrono::TimeZone;
    use sqlx::{PgConnection, PgPool};
    use std::time::Instant;

    async fn setup_with_connection(
        pool: PgPool,
    ) -> Result<(World, EntityId, Receiver<EcsMessage>, Account)> {
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

        Ok((world, connection_global_world_id, rx_channel, account))
    }

    fn assemble_create_user_packet() -> CCreateUser {
        CCreateUser {
            name: "testuser".to_string(),
            details: vec![
                13, 19, 26, 8, 0, 0, 0, 0, 31, 10, 4, 0, 23, 10, 0, 0, 9, 0, 12, 13, 0, 0, 0, 0,
                21, 31, 14, 22, 29, 16, 16, 0,
            ],
            shape: vec![
                1, 19, 16, 19, 19, 16, 19, 19, 19, 15, 15, 15, 15, 15, 15, 15, 16, 19, 10, 0, 5,
                11, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            gender: Gender::Female,
            race: Race::Aman,
            class: Class::Warrior,
            appearance: Customization(vec![101, 30, 11, 1, 9, 25, 4, 0]),
            is_second_character: false,
            appearance2: 100,
        }
    }

    async fn create_user(conn: &mut PgConnection, account_id: i64, num: i32) -> Result<User> {
        Ok(user::create(
            conn,
            &User {
                id: -1,
                account_id,
                name: format!("name-{}", num),
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
                lobby_slot: num,
                is_new_character: false,
                tutorial_state: 0,
                is_deleting: false,
                delete_at: None,
                last_logout_at: Utc.ymd(2007, 7, 8).and_hms(9, 10, 11),
                created_at: Utc.ymd(2009, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?)
    }

    #[test]
    fn test_can_create_user_true() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, rx_channel, _account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestCanCreateUser {
                            connection_global_world_id,
                            account_id: -1,
                            packet: CCanCreateUser {},
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCanCreateUser { packet, .. } => {
                        assert!(packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCanCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            Ok(())
        })
    }

    #[test]
    fn test_can_create_user_false() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            for i in 0..MAX_USERS_PER_ACCOUNT as i32 {
                task::block_on(async { create_user(&mut conn, account.id, i).await })?;
            }

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestCanCreateUser {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CCanCreateUser {},
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCanCreateUser { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCanCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            Ok(())
        })
    }

    #[test]
    fn test_is_valid_user_name() {
        // Valid user names
        assert!(is_valid_user_name("Simple"));
        assert!(is_valid_user_name("Simple123"));
        assert!(is_valid_user_name("654562312"));

        // Invalid user names
        assert!(!is_valid_user_name("Simp le"));
        assert!(!is_valid_user_name("Simple!"));
        assert!(!is_valid_user_name("Simple "));
        assert!(!is_valid_user_name(" Simple"));
        assert!(!is_valid_user_name("´test`"));
        assert!(!is_valid_user_name(""));
        assert!(!is_valid_user_name(" "));
        assert!(!is_valid_user_name("\n"));
        assert!(!is_valid_user_name("\t"));
        assert!(!is_valid_user_name("기브스"));
        assert!(!is_valid_user_name("ダース"));
        assert!(!is_valid_user_name("การเดินทาง"));
        assert!(!is_valid_user_name("العربية"));
    }

    #[test]
    fn test_check_user_name_available() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    for i in 0..5 {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestCheckUserName {
                                connection_global_world_id,
                                account_id: account.id,
                                packet: CCheckUserName {
                                    name: format!("NotTakenUserName{}", i),
                                },
                            }),
                        );
                    }
                },
            );

            world.run(user_manager_system);

            let mut count = 0;
            loop {
                if let Ok(message) = rx_channel.try_recv() {
                    match *message {
                        Message::ResponseCheckUserName { packet, .. } => {
                            if packet.ok {
                                count += 1
                            }
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
            assert_eq!(count, 5);

            Ok(())
        })
    }

    #[test]
    fn test_check_user_name_invalid_username() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestCheckUserName {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CCheckUserName {
                                name: "H!x?or{}".to_string(),
                            },
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCheckUserName { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCheckUserName message"),
                }
            } else {
                panic!("Can't find any message");
            }

            Ok(())
        })
    }

    #[test]
    fn test_get_user_list() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            for i in 0..MAX_USERS_PER_ACCOUNT as i32 {
                task::block_on(async { create_user(&mut conn, account.id, i).await })?;
            }

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestGetUserList {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CGetUserList {},
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            let expected_packet_count = if MAX_USERS_PER_ACCOUNT % CHUNK_SIZE != 0 {
                (MAX_USERS_PER_ACCOUNT / CHUNK_SIZE) + 1
            } else {
                MAX_USERS_PER_ACCOUNT / CHUNK_SIZE
            };

            let mut char_count = 0;
            let mut packet_count = 0;
            loop {
                if let Ok(message) = rx_channel.try_recv() {
                    packet_count += 1;
                    match *message {
                        Message::ResponseGetUserList { packet, .. } => {
                            char_count += packet.characters.len();

                            if packet_count == 1 {
                                // First page
                                assert_eq!(packet.first, true);
                                assert_eq!(packet.more, true);
                            } else if packet_count == expected_packet_count {
                                // Last page
                                assert_eq!(packet.first, false);
                                assert_eq!(packet.more, false);
                            } else {
                                // In between
                                assert_eq!(packet.first, false);
                                assert_eq!(packet.more, true);
                            }
                        }
                        _ => panic!("Received an unexpected message: {}", message),
                    }
                } else {
                    break;
                }
            }

            assert_eq!(char_count, MAX_USERS_PER_ACCOUNT);
            assert_eq!(packet_count, expected_packet_count);

            Ok(())
        })
    }

    #[test]
    fn test_get_empty_user_list() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestGetUserList {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CGetUserList {},
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            let mut char_count = 0;
            let mut packet_count = 0;
            loop {
                if let Ok(message) = rx_channel.try_recv() {
                    packet_count += 1;
                    match *message {
                        Message::ResponseGetUserList { packet, .. } => {
                            char_count = packet.characters.len()
                        }
                        _ => panic!("Received an unexpected message: {}", message),
                    }
                } else {
                    break;
                }
            }

            assert_eq!(char_count, 0);
            assert_eq!(packet_count, 1);

            Ok(())
        })
    }

    #[test]
    fn test_create_user_successful() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            let org_packet = assemble_create_user_packet();

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestCreateUser {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: org_packet.clone(),
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCreateUser { packet, .. } => {
                        assert!(packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            let mut users: Vec<User> =
                task::block_on(async { user::list(&mut conn, account.id).await })?;

            if let Some(u) = users.pop() {
                assert_eq!(u.name, org_packet.name);
                assert_eq!(u.details, org_packet.details);
                assert_eq!(u.shape, org_packet.shape);
                assert_eq!(u.gender, org_packet.gender);
                assert_eq!(u.race, org_packet.race);
                assert_eq!(u.class, org_packet.class);
                assert_eq!(u.appearance, org_packet.appearance);
                assert_eq!(u.appearance2, org_packet.appearance2);
            } else {
                panic!("Can't find the created user");
            }

            // TODO test for user_location

            Ok(())
        })
    }

    #[test]
    fn test_create_user_unsuccessful_name_taken() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            let org_packet = assemble_create_user_packet();

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    for _i in 0..2 {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestCreateUser {
                                connection_global_world_id,
                                account_id: account.id,
                                packet: org_packet.clone(),
                            }),
                        );
                    }
                },
            );

            world.run(user_manager_system);

            // First user could be created
            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCreateUser { packet, .. } => {
                        assert!(packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            // Second user failed because the name was already taken
            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCreateUser { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            let count =
                task::block_on(async { user::get_user_count(&mut conn, account.id).await })?;
            assert_eq!(count, 1);

            Ok(())
        })
    }

    #[test]
    fn test_create_user_unsuccessful_no_slots_left() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            for i in 0..MAX_USERS_PER_ACCOUNT as i32 {
                task::block_on(async { create_user(&mut conn, account.id, i).await })?;
            }

            let org_packet = assemble_create_user_packet();

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    for _i in 0..2 {
                        entities.add_entity(
                            &mut messages,
                            Box::new(Message::RequestCreateUser {
                                connection_global_world_id,
                                account_id: account.id,
                                packet: org_packet.clone(),
                            }),
                        );
                    }
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseCreateUser { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Message is not a ResponseCreateUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            let count =
                task::block_on(async { user::get_user_count(&mut conn, account.id).await })?;
            assert_eq!(count, MAX_USERS_PER_ACCOUNT as i64);

            Ok(())
        })
    }

    #[test]
    fn test_delete_user() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            let mut users: Vec<User> = Vec::new();
            task::block_on(async {
                for i in 0..MAX_USERS_PER_ACCOUNT as i32 {
                    let user: User = create_user(&mut conn, account.id, i).await.unwrap();
                    users.push(user);
                }
            });

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestDeleteUser {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CDeleteUser {
                                database_id: users[0].id,
                            },
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(message) = rx_channel.try_recv() {
                match *message {
                    Message::ResponseDeleteUser { packet, .. } => {
                        assert!(packet.ok);
                    }
                    _ => panic!("Message is not a ResponseDeleteUser message"),
                }
            } else {
                panic!("Can't find any message");
            }

            users = task::block_on(async { user::list(&mut conn, account.id).await })?;

            for i in 0..(MAX_USERS_PER_ACCOUNT - 1) {
                if let Some(u) = users.get(i) {
                    assert_eq!(u.lobby_slot, (i + 1) as i32);
                    assert_eq!(u.name, format!("name-{}", i + 1))
                } else {
                    panic!("Can't find user in position {}", i);
                }
            }

            // TODO Test for user_location

            Ok(())
        })
    }

    #[test]
    fn test_change_user_lobby_slot_id() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_global_world_id, _rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            let mut users: Vec<User> = Vec::new();
            task::block_on(async {
                for i in 0..MAX_USERS_PER_ACCOUNT as i32 {
                    let user: User = create_user(&mut conn, account.id, i + 1).await.unwrap();
                    users.push(user);
                }
            });

            users.reverse();

            let user_positions: Vec<CChangeUserLobbySlotIdEntry> = users
                .iter()
                .map(|u| CChangeUserLobbySlotIdEntry {
                    database_id: u.id,
                    lobby_slot: (MAX_USERS_PER_ACCOUNT as i32 - u.lobby_slot + 1),
                })
                .collect();

            world.run(
                |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestChangeUserLobbySlotId {
                            connection_global_world_id,
                            account_id: account.id,
                            packet: CChangeUserLobbySlotId { user_positions },
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            users = task::block_on(async { user::list(&mut conn, account.id).await })?;

            for i in 0..MAX_USERS_PER_ACCOUNT {
                if let Some(u) = users.get(i) {
                    assert_eq!(u.lobby_slot, (i + 1) as i32);
                    assert_eq!(u.name, format!("name-{}", MAX_USERS_PER_ACCOUNT - i))
                } else {
                    panic!("Can't find user in position {}", i);
                }
            }

            Ok(())
        })
    }
}
