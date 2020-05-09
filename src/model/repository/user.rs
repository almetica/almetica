/// Handles the users of an account (the characters).
use crate::model::entity::User;
use crate::Result;
use sqlx::prelude::*;
use sqlx::PgConnection;

// TODO get user count for account

/// Creates a new user.
pub async fn create(conn: &mut PgConnection, user: &User) -> Result<User> {
    Ok(sqlx::query_as(
        r#"INSERT INTO "user"
        VALUES (DEFAULT, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, DEFAULT, DEFAULT)
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
    .bind(&user.world_id)
    .bind(&user.guard_id)
    .bind(&user.section_id)
    .bind(&user.level)
    .bind(&user.awakening_level)
    .bind(&user.laurel)
    .bind(&user.achievement_points)
    .bind(&user.playtime)
    .bind(&user.rest_bonus_xp)
    .bind(&user.show_face)
    .bind(&user.show_style)
    .bind(&user.position)
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
        r#"UPDATE account_user SET
                    name = $1,
                    gender = $2,
                    race = $3,
                    class = $4,
                    shape = $5,
                    details = $6,
                    appearance = $7,
                    appearance2 = $8,
                    world_id = $9,
                    guard_id = $10,
                    section_id = $11,
                    level = $12,
                    awakening_level = $13,
                    laurel = $14,
                    achievement_points = $15,
                    playtime = $16,
                    rest_bonus_xp = $17,
                    show_face = $18,
                    show_style = $19,
                    position = $20,
                    is_new_character = $21,
                    tutorial_state = $22,
                    is_deleting = $23,
                    delete_at = $24,
                    last_logout_at = $25,
                    WHERE id = $26,
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
    .bind(&user.world_id)
    .bind(&user.guard_id)
    .bind(&user.section_id)
    .bind(&user.level)
    .bind(&user.awakening_level)
    .bind(&user.laurel)
    .bind(&user.achievement_points)
    .bind(&user.playtime)
    .bind(&user.rest_bonus_xp)
    .bind(&user.show_face)
    .bind(&user.show_style)
    .bind(&user.position)
    .bind(&user.is_new_character)
    .bind(&user.tutorial_state)
    .bind(&user.is_deleting)
    .bind(&user.delete_at)
    .bind(&user.last_logout_at)
    .bind(&user.id)
    .fetch_one(conn)
    .await?)
}

/// Updates the position of an user with the given ID.
pub async fn update_position(conn: &mut PgConnection, id: i32, position: i32) -> Result<()> {
    sqlx::query("UPDATE account_user SET position = $1 WHERE id = $2")
        .bind(&position)
        .bind(&id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Finds an user by id.
pub async fn get_by_id(conn: &mut PgConnection, id: i64) -> Result<User> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM user WHERE id = $1")
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
}

/// Checks if an user with the given name already exists.
pub async fn is_user_name_taken(conn: &mut PgConnection, name: &str) -> Result<bool> {
    let found = sqlx::query("SELECT 1 FROM account_user WHERE name=$1")
        .bind(name)
        .execute(conn)
        .await?;
    if found == 0 {
        Ok(false)
    } else {
        Ok(true)
    }
}

/// Deletes an user with the given id.
pub async fn delete_by_id(conn: &mut PgConnection, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM account_user WHERE id = $1")
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
    use chrono::prelude::*;
    use sqlx::PgPool;

    async fn create_account(pool: &mut PgConnection) -> Result<Account> {
        let account = Account {
            id: -1,
            name: "testuser".to_string(),
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
            name: format!("testpassword-{}", num),
            gender: Gender::Female,
            race: Race::Human,
            class: Class::Warrior,
            shape: vec![0u8],
            details: vec![0u8],
            appearance: Customization(vec![0u8]),
            appearance2: 0,
            world_id: 0,
            guard_id: 0,
            section_id: 0,
            level: 1,
            awakening_level: 0,
            laurel: 0,
            achievement_points: 0,
            playtime: 0,
            rest_bonus_xp: 0,
            show_face: false,
            show_style: false,
            position: 0,
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
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();
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
            assert_eq!(org_user.world_id, db_user.world_id);
            assert_eq!(org_user.guard_id, db_user.guard_id);
            assert_eq!(org_user.section_id, db_user.section_id);
            assert_eq!(org_user.level, db_user.level);
            assert_eq!(org_user.awakening_level, db_user.awakening_level);
            assert_eq!(org_user.laurel, db_user.laurel);
            assert_eq!(org_user.achievement_points, db_user.achievement_points);
            assert_eq!(org_user.playtime, db_user.playtime);
            assert_eq!(org_user.rest_bonus_xp, db_user.rest_bonus_xp);
            assert_eq!(org_user.show_face, db_user.show_face);
            assert_eq!(org_user.show_style, db_user.show_style);
            assert_eq!(org_user.position, db_user.position);
            assert_eq!(org_user.is_new_character, db_user.is_new_character);
            assert_eq!(org_user.tutorial_state, db_user.tutorial_state);
            assert_eq!(org_user.is_deleting, db_user.is_deleting);
            assert_eq!(org_user.delete_at, db_user.delete_at);
            assert_ne!(org_user.last_logout_at, db_user.last_logout_at);
            assert_ne!(org_user.created_at, db_user.created_at);

            Ok(())
        }
        db_test(test)
    }

    // TODO all other tests
}
