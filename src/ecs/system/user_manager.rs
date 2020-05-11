/// Handles the users of an account. Users in TERA terminology are the player characters of an account.
use crate::ecs::component::Connection;
use crate::ecs::event::Event::ResponseGetUserList;
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::WorldId;
use crate::ecs::system::send_event;
use crate::model::entity::User;
use crate::model::repository::user;
use crate::model::{Vec3, Vec3a};
use crate::protocol::packet::*;
use crate::Result;
use anyhow::Context;
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

pub fn user_manager_system(
    incoming_events: View<EcsEvent>,
    connections: View<Connection>,
    pool: UniqueView<PgPool>,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    // TODO Look for users without a connection component. Set their "deletion time" and persist them then.
    (&incoming_events).iter().for_each(|event| match &**event {
        Event::RequestCanCreateUser {
            connection_id,
            account_id,
            ..
        } => {
            if let Err(e) = handle_can_create_user(*connection_id, *account_id, &connections, &pool)
            {
                error!("Rejecting create user request: {:?}", e);
                send_event(
                    assemble_can_create_user_response(*connection_id, false),
                    &connections,
                );
            }
        }
        Event::RequestGetUserList {
            connection_id,
            account_id,
            ..
        } => {
            if let Err(e) = handle_user_list(*connection_id, *account_id, &connections, &pool) {
                error!("Rejecting get user list request: {:?}", e);
                send_event(
                    assemble_user_list_response(*connection_id, &Vec::new(), true, true),
                    &connections,
                );
            }
        }
        Event::RequestCheckUserName {
            connection_id,
            packet,
            ..
        } => {
            if let Err(e) = handle_check_user_name(&packet, *connection_id, &connections, &pool) {
                error!("Rejecting check user name request: {:?}", e);
                send_event(
                    assemble_check_user_name_response(*connection_id, false),
                    &connections,
                );
            }
        }
        Event::RequestCreateUser {
            connection_id,
            account_id,
            packet,
        } => {
            if let Err(e) =
                handle_create_user(&packet, *connection_id, *account_id, &connections, &pool)
            {
                error!("Rejecting create user request: {:?}", e);
                send_event(
                    assemble_create_user_response(*connection_id, false),
                    &connections,
                );
            }
        }
        _ => { /* Ignore all other events */ }
    });
}

fn handle_user_list(
    connection_id: EntityId,
    account_id: i64,
    connections: &View<Connection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!("Get user list event incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        // Send the user list paged, since we can only send 16kiB of data in one packet
        let mut is_first_page = true;

        let users = user::list(&mut conn, account_id).await?;

        if users.len() == 0 {
            send_event(
                assemble_user_list_response(connection_id, &Vec::new(), true, true),
                connections,
            );
        } else {
            for chunk in users.chunks(CHUNK_SIZE) {
                let is_last_page = if chunk.len() == CHUNK_SIZE {
                    false
                } else {
                    true
                };

                send_event(
                    assemble_user_list_response(connection_id, chunk, is_first_page, is_last_page),
                    connections,
                );

                is_first_page = false;
            }
        }

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_can_create_user(
    connection_id: EntityId,
    account_id: i64,
    connections: &View<Connection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!("Can create user event incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if can_create_user(&mut conn, account_id).await? {
            send_event(
                assemble_can_create_user_response(connection_id, true),
                connections,
            );
        } else {
            send_event(
                assemble_can_create_user_response(connection_id, false),
                connections,
            );
        }

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_create_user(
    packet: &CCreateUser,
    connection_id: EntityId,
    account_id: i64,
    connections: &View<Connection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!("Create user event incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .begin()
            .await
            .context("Couldn't acquire connection from pool")?;

        // TODO validate the character even more

        if can_create_user(&mut conn, account_id).await?
            && check_username(&mut conn, &packet.name).await?
        {
            let next_position = user::get_user_count(&mut conn, account_id).await? + 1;
            create_new_user(&mut conn, account_id, next_position as i32, packet).await?;
            send_event(
                assemble_create_user_response(connection_id, true),
                connections,
            );
        } else {
            send_event(
                assemble_create_user_response(connection_id, false),
                connections,
            );
        }

        conn.commit().await?;

        Ok::<(), anyhow::Error>(())
    })?)
}

fn handle_check_user_name(
    packet: &CCheckUserName,
    connection_id: EntityId,
    connections: &View<Connection>,
    pool: &UniqueView<PgPool>,
) -> Result<()> {
    let span = info_span!("connection", connection = ?connection_id);
    let _enter = span.enter();

    debug!("Check user name event incoming");

    Ok(task::block_on(async {
        let mut conn = pool
            .acquire()
            .await
            .context("Couldn't acquire connection from pool")?;

        if check_username(&mut conn, &packet.name).await? {
            send_event(
                assemble_check_user_name_response(connection_id, true),
                connections,
            );
        } else {
            send_event(
                assemble_check_user_name_response(connection_id, false),
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
    position: i32,
    packet: &CCreateUser,
) -> Result<()> {
    // TODO set the tutorial map as the start point
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
            world_id: 1,
            guard_id: 2,
            section_id: 8,
            level: 1,
            awakening_level: 0,
            laurel: 0,
            achievement_points: 0,
            playtime: 0,
            rest_bonus_xp: 0,
            show_face: false,
            show_style: false,
            position,
            is_new_character: true,
            tutorial_state: 0,
            is_deleting: false,
            delete_at: None,
            last_logout_at: Utc::now(),
            created_at: Utc::now(),
        },
    )
    .await?;
    Ok(())
}

/// Only alphanumeric characters are currently allowed. The client in rather limited with it's font.
fn is_valid_user_name(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"^[[:alnum:]]+$"#).unwrap();
    }
    RE.is_match(text)
}

fn assemble_can_create_user_response(connection_id: EntityId, ok: bool) -> EcsEvent {
    Box::new(Event::ResponseCanCreateUser {
        connection_id,
        packet: SCanCreateUser { ok },
    })
}

fn assemble_create_user_response(connection_id: EntityId, ok: bool) -> EcsEvent {
    Box::new(Event::ResponseCreateUser {
        connection_id,
        packet: SCreateUser { ok },
    })
}

fn assemble_check_user_name_response(connection_id: EntityId, ok: bool) -> EcsEvent {
    Box::new(Event::ResponseCheckUserName {
        connection_id,
        packet: SCheckUserName { ok },
    })
}

fn assemble_user_list_response(
    connection_id: EntityId,
    users: &[User],
    is_first_page: bool,
    is_last_page: bool,
) -> EcsEvent {
    // TODO calculate hp/mp/max_rest_bonus and also return the equip / styles / custom strings / guild / has_broker_sales from db
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
                world_id: user.world_id,
                guard_id: user.guard_id,
                section_id: user.section_id,
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
                position: user.position,
                guild_logo_id: 0,
                awakening_level: user.awakening_level,
                has_broker_sales: false,
            }
        })
        .collect();

    Box::new(ResponseGetUserList {
        connection_id,
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
    use crate::ecs::component::Connection;
    use crate::ecs::event::Event;
    use crate::model::entity::Account;
    use crate::model::repository::account;
    use crate::model::tests::db_test;
    use crate::model::{Class, Gender, PasswordHashAlgorithm, Race};
    use crate::Result;
    use async_std::sync::{channel, Receiver};
    use chrono::TimeZone;
    use sqlx::{PgConnection, PgPool};
    use std::time::Instant;

    async fn setup_with_connection(
        pool: PgPool,
    ) -> Result<(World, EntityId, Receiver<EcsEvent>, Account)> {
        let mut conn = pool.acquire().await?;

        let world = World::new();
        world.add_unique(WorldId(0));
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

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        channel: tx_channel,
                        account_id: Some(account.id),
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        Ok((world, connection_id, rx_channel, account))
    }

    async fn create_user(conn: &mut PgConnection, account_id: i64, num: usize) -> Result<()> {
        user::create(
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
                world_id: 0,
                guard_id: 0,
                section_id: 0,
                level: 0,
                awakening_level: 0,
                laurel: 0,
                achievement_points: 0,
                playtime: 0,
                rest_bonus_xp: 0,
                show_face: false,
                show_style: false,
                position: 0,
                is_new_character: false,
                tutorial_state: 0,
                is_deleting: false,
                delete_at: None,
                last_logout_at: Utc.ymd(2007, 7, 8).and_hms(9, 10, 11),
                created_at: Utc.ymd(2009, 7, 8).and_hms(9, 10, 11),
            },
        )
        .await?;
        Ok(())
    }

    #[test]
    fn test_can_create_user_true() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let pool = PgPool::new(db_string).await?;
                let (world, connection_id, rx_channel, _account) =
                    setup_with_connection(pool).await?;

                task::spawn_blocking(move || {
                    world.run(
                        |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                            entities.add_entity(
                                &mut events,
                                Box::new(Event::RequestCanCreateUser {
                                    connection_id,
                                    account_id: -1,
                                    packet: CCanCreateUser {},
                                }),
                            );
                        },
                    );

                    world.run(user_manager_system);

                    if let Ok(event) = rx_channel.try_recv() {
                        match *event {
                            Event::ResponseCanCreateUser { packet, .. } => {
                                assert!(packet.ok);
                            }
                            _ => panic!("Event is not a ResponseCanCreateUser event."),
                        }
                    } else {
                        panic!("Can't find any event.");
                    }
                    Ok::<(), anyhow::Error>(())
                })
                .await?;

                Ok(())
            })
        })
    }

    #[test]
    fn test_can_create_user_false() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            for i in 0..20 {
                task::block_on(async { create_user(&mut conn, account.id, i).await })?;
            }

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestCanCreateUser {
                            connection_id,
                            account_id: account.id,
                            packet: CCanCreateUser {},
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(event) = rx_channel.try_recv() {
                match *event {
                    Event::ResponseCanCreateUser { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Event is not a ResponseCanCreateUser event."),
                }
            } else {
                panic!("Can't find any event.");
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
            let (world, connection_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    for i in 0..5 {
                        entities.add_entity(
                            &mut events,
                            Box::new(Event::RequestCheckUserName {
                                connection_id,
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
                if let Ok(event) = rx_channel.try_recv() {
                    match *event {
                        Event::ResponseCheckUserName { packet, .. } => {
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
            let (world, connection_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestCheckUserName {
                            connection_id,
                            account_id: account.id,
                            packet: CCheckUserName {
                                name: "H!x?or{}".to_string(),
                            },
                        }),
                    );
                },
            );

            world.run(user_manager_system);

            if let Ok(event) = rx_channel.try_recv() {
                match *event {
                    Event::ResponseCheckUserName { packet, .. } => {
                        assert!(!packet.ok);
                    }
                    _ => panic!("Event is not a ResponseCheckUserName event."),
                }
            } else {
                panic!("Can't find any event.");
            }

            Ok(())
        })
    }

    #[test]
    fn test_get_user_list() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let mut conn = task::block_on(async { pool.acquire().await })?;
            let (world, connection_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            for i in 0..MAX_USERS_PER_ACCOUNT {
                task::block_on(async { create_user(&mut conn, account.id, i).await })?;
            }

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestGetUserList {
                            connection_id,
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
                if let Ok(event) = rx_channel.try_recv() {
                    packet_count += 1;
                    match *event {
                        Event::ResponseGetUserList { packet, .. } => {
                            char_count += packet.characters.len()
                        }
                        _ => panic!("Received an unexpected event: {}", event),
                    }
                } else {
                    break;
                }
            }

            let expected_packet_count = if MAX_USERS_PER_ACCOUNT % CHUNK_SIZE != 0 {
                (MAX_USERS_PER_ACCOUNT / CHUNK_SIZE) + 1
            } else {
                MAX_USERS_PER_ACCOUNT / CHUNK_SIZE
            };

            assert_eq!(char_count, MAX_USERS_PER_ACCOUNT);
            assert_eq!(packet_count, expected_packet_count);

            Ok(())
        })
    }

    #[test]
    fn test_get_empty_user_list() -> Result<()> {
        db_test(|db_string| {
            let pool = task::block_on(async { PgPool::new(db_string).await })?;
            let (world, connection_id, rx_channel, account) =
                task::block_on(async { setup_with_connection(pool).await })?;

            world.run(
                |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestGetUserList {
                            connection_id,
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
                if let Ok(event) = rx_channel.try_recv() {
                    packet_count += 1;
                    match *event {
                        Event::ResponseGetUserList { packet, .. } => {
                            char_count = packet.characters.len()
                        }
                        _ => panic!("Received an unexpected event: {}", event),
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

    // TODO write a handle_create_user test (valid, 2x invalid)
}
