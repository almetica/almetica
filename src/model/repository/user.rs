/// Handles the users of an account (the characters).
use crate::model::entity::User;
use crate::Result;
use sqlx::prelude::*;
use sqlx::PgConnection;

/// Creates a new user.
pub async fn create(conn: &mut PgConnection, user: &User) -> Result<User> {
    Ok(sqlx::query_as(
        r#"INSERT INTO "user"
        VALUES (DEFAULT, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, DEFAULT, DEFAULT)
        RETURNING *"#,
    )
    .bind(&user.account_id)
    .bind(&user.name)
    .bind(&user.gender)
    .bind(&user.race)
    .bind(&user.class)
    .bind(&user.shape)
    .bind(&user.details)
    .bind(&user.appearance)
    .bind(&user.appearance2)
    .bind(&user.level)
    .bind(&user.awakening_level)
    .bind(&user.laurel)
    .bind(&user.achievement_points)
    .bind(&user.playtime)
    .bind(&user.rest_bonus_xp)
    .bind(&user.show_face)
    .bind(&user.show_style)
    .bind(&user.lobby_slot)
    .bind(&user.is_new_character)
    .bind(&user.tutorial_state)
    .bind(&user.is_deleting)
    .bind(&user.delete_at)
    .fetch_one(conn)
    .await?)
}

/// Updates an user.
pub async fn update(conn: &mut PgConnection, user: &User) -> Result<User> {
    Ok(sqlx::query_as(
        r#"UPDATE "user" SET
            "name" = $1,
            "gender" = $2,
            "race" = $3,
            "class" = $4,
            "shape" = $5,
            "details" = $6,
            "appearance" = $7,
            "appearance2" = $8,
            "level" = $9,
            "awakening_level" = $10,
            "laurel" = $11,
            "achievement_points" = $12,
            "playtime" = $13,
            "rest_bonus_xp" = $14,
            "show_face" = $15,
            "show_style" = $16,
            "lobby_slot" = $17,
            "is_new_character" = $18,
            "tutorial_state" = $19,
            "is_deleting" = $20,
            "delete_at" = $21,
            "last_logout_at" = $22
            WHERE "id" = $23
            RETURNING *"#,
    )
    .bind(&user.name)
    .bind(&user.gender)
    .bind(&user.race)
    .bind(&user.class)
    .bind(&user.shape)
    .bind(&user.details)
    .bind(&user.appearance)
    .bind(&user.appearance2)
    .bind(&user.level)
    .bind(&user.awakening_level)
    .bind(&user.laurel)
    .bind(&user.achievement_points)
    .bind(&user.playtime)
    .bind(&user.rest_bonus_xp)
    .bind(&user.show_face)
    .bind(&user.show_style)
    .bind(&user.lobby_slot)
    .bind(&user.is_new_character)
    .bind(&user.tutorial_state)
    .bind(&user.is_deleting)
    .bind(&user.delete_at)
    .bind(&user.last_logout_at)
    .bind(&user.id)
    .fetch_one(conn)
    .await?)
}

/// Updates the lobby_slot of an user with the given ID.
pub async fn update_lobby_slot(conn: &mut PgConnection, id: i32, position: i32) -> Result<()> {
    sqlx::query(r#"UPDATE "user" SET "lobby_slot" = $1 WHERE "id" = $2"#)
        .bind(&position)
        .bind(&id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Finds an user by id.
pub async fn get_by_id(conn: &mut PgConnection, id: i32) -> Result<User> {
    Ok(
        sqlx::query_as::<_, User>(r#"SELECT * FROM "user" WHERE "id" = $1"#)
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
}

/// Get the user count of an account.
pub async fn get_user_count(conn: &mut PgConnection, account_id: i64) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as(r#"SELECT COUNT(1) FROM "user" WHERE "account_id" = $1"#)
        .bind(account_id)
        .fetch_one(conn)
        .await?;
    Ok(count)
}

/// Get all users of an account.
pub async fn list(conn: &mut PgConnection, account_id: i64) -> Result<Vec<User>> {
    Ok(
        sqlx::query_as(r#"SELECT * FROM "user" WHERE "account_id" = $1 ORDER BY "lobby_slot""#)
            .bind(account_id)
            .fetch_all(conn)
            .await?,
    )
}

/// Checks if an user with the given name already exists.
pub async fn is_user_name_taken(conn: &mut PgConnection, name: &str) -> Result<bool> {
    let (found,): (bool,) =
        sqlx::query_as(r#"SELECT EXISTS(SELECT 1 FROM "user" WHERE "name" = $1)"#)
            .bind(name)
            .fetch_one(conn)
            .await?;
    Ok(found)
}

/// Deletes an user with the given id.
pub async fn delete_by_id(conn: &mut PgConnection, id: i32) -> Result<()> {
    sqlx::query(r#"DELETE FROM "user" WHERE "id" = $1"#)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::entity::{Account, User};
    use crate::model::repository::account;
    use crate::model::tests::db_test;
    use crate::model::{Class, Customization, Gender, PasswordHashAlgorithm, Race};
    use crate::Result;
    use async_std::task;
    use chrono::prelude::*;
    use sqlx::PgConnection;

    async fn create_account(pool: &mut PgConnection) -> Result<Account> {
        let account = Account {
            id: -1,
            name: "testaccount".to_string(),
            password: "not-a-real-password-hash".to_string(),
            algorithm: PasswordHashAlgorithm::Argon2,
            created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
        };
        Ok(account::create(pool, &account).await?)
    }

    fn get_default_user(account: &Account, num: i32) -> User {
        User {
            id: -1,
            account_id: account.id,
            name: format!("testuser-{}", num),
            gender: Gender::Female,
            race: Race::Human,
            class: Class::Warrior,
            shape: vec![0u8],
            details: vec![0u8],
            appearance: Customization(vec![0u8]),
            appearance2: 0,
            level: 1,
            awakening_level: 0,
            laurel: 0,
            achievement_points: 0,
            playtime: 0,
            rest_bonus_xp: 0,
            show_face: false,
            show_style: false,
            lobby_slot: num,
            is_new_character: true,
            tutorial_state: 0,
            is_deleting: false,
            delete_at: None,
            last_logout_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
        }
    }

    #[test]
    fn test_create_user() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let org_user = get_default_user(&account, 0);

                let db_user = create(&mut conn, &org_user).await?;

                assert_ne!(org_user.id, db_user.id);
                assert_eq!(org_user.account_id, db_user.account_id);
                assert_eq!(org_user.name, db_user.name);
                assert_eq!(org_user.gender, db_user.gender);
                assert_eq!(org_user.race, db_user.race);
                assert_eq!(org_user.class, db_user.class);
                assert_eq!(org_user.shape, db_user.shape);
                assert_eq!(org_user.details, db_user.details);
                assert_eq!(org_user.appearance, db_user.appearance);
                assert_eq!(org_user.appearance2, db_user.appearance2);
                assert_eq!(org_user.level, db_user.level);
                assert_eq!(org_user.awakening_level, db_user.awakening_level);
                assert_eq!(org_user.laurel, db_user.laurel);
                assert_eq!(org_user.achievement_points, db_user.achievement_points);
                assert_eq!(org_user.playtime, db_user.playtime);
                assert_eq!(org_user.rest_bonus_xp, db_user.rest_bonus_xp);
                assert_eq!(org_user.show_face, db_user.show_face);
                assert_eq!(org_user.show_style, db_user.show_style);
                assert_eq!(org_user.lobby_slot, db_user.lobby_slot);
                assert_eq!(org_user.is_new_character, db_user.is_new_character);
                assert_eq!(org_user.tutorial_state, db_user.tutorial_state);
                assert_eq!(org_user.is_deleting, db_user.is_deleting);
                assert_eq!(org_user.delete_at, db_user.delete_at);
                assert_ne!(org_user.last_logout_at, db_user.last_logout_at);
                assert_ne!(org_user.created_at, db_user.created_at);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_user() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let mut db_user = create(&mut conn, &get_default_user(&account, 0)).await?;
                let org_user = db_user.clone();

                assert_ne!(db_user.id, -1);

                db_user.account_id = 12312;
                db_user.name = "new_user_name".to_string();
                db_user.class = Class::Archer;
                db_user.created_at = Utc.ymd(2003, 7, 8).and_hms(11, 40, 20);

                let updated_db_user = update(&mut conn, &db_user).await?;

                assert_eq!(updated_db_user.id, db_user.id);
                assert_eq!(updated_db_user.account_id, org_user.account_id);
                assert_ne!(updated_db_user.name, org_user.name);
                assert_eq!(updated_db_user.name, "new_user_name");
                assert_ne!(updated_db_user.class, org_user.class);
                assert_eq!(updated_db_user.class, Class::Archer);
                assert_eq!(updated_db_user.created_at, org_user.created_at);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_user_position() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let db_user = create(&mut conn, &get_default_user(&account, 0)).await?;
                assert_ne!(db_user.id, -1);

                update_lobby_slot(&mut conn, db_user.id, 15).await?;
                let updated_db_user = get_by_id(&mut conn, db_user.id).await?;

                assert_ne!(updated_db_user.lobby_slot, 0);
                assert_eq!(updated_db_user.lobby_slot, 15);
                assert_eq!(updated_db_user.id, db_user.id);
                assert_eq!(updated_db_user.account_id, db_user.account_id);
                assert_eq!(updated_db_user.created_at, db_user.created_at);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_get_by_id() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let db_user = create(&mut conn, &get_default_user(&account, 0)).await?;
                assert_ne!(db_user.id, -1);

                let updated_db_user = get_by_id(&mut conn, db_user.id).await?;

                assert_eq!(updated_db_user.id, db_user.id);
                assert_eq!(updated_db_user.account_id, db_user.account_id);
                assert_eq!(updated_db_user.created_at, db_user.created_at);

                Ok(())
            })
        })
    }

    #[test]
    fn test_list_users() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;

                for i in 1..=10i32 {
                    create(&mut conn, &get_default_user(&account, i)).await?;
                }
                let users = list(&mut conn, account.id).await?;

                assert_eq!(users.len(), 10);

                Ok(())
            })
        })
    }

    #[test]
    fn test_get_user_count() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;

                for i in 1..=10i32 {
                    create(&mut conn, &get_default_user(&account, i)).await?;
                }
                let count = get_user_count(&mut conn, account.id).await?;

                assert_eq!(count, 10);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_is_user_name_taken() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let db_user = create(&mut conn, &get_default_user(&account, 99)).await?;
                assert_ne!(db_user.id, -1);

                assert!(is_user_name_taken(&mut conn, "testuser-99").await?);
                assert!(!is_user_name_taken(&mut conn, "not-taken").await?);

                Ok(())
            })
        })
    }

    #[test]
    fn test_delete_user() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let account = create_account(&mut conn).await?;
                let db_user = create(&mut conn, &get_default_user(&account, 99)).await?;
                assert_ne!(db_user.id, -1);

                delete_by_id(&mut conn, db_user.id).await?;

                match get_by_id(&mut conn, db_user.id).await {
                    Ok(..) => panic!("Found user that we expected to delete"),
                    Err(e) => match e.downcast_ref::<sqlx::Error>() {
                        Some(sqlx::Error::RowNotFound) => { /* Expected result */ }
                        Some(..) | None => panic!(e),
                    },
                }
                Ok(())
            })
        })
    }
}
