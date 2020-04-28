/// Handles the accounts of the player.
use sqlx::prelude::*;
use sqlx::PgConnection;

use crate::model::entity::Account;
use crate::model::PasswordHashAlgorithm;
use crate::Result;

/// Creates an new account.
pub async fn create(conn: &mut PgConnection, account: &Account) -> Result<Account> {
    Ok(sqlx::query_as::<_, Account>(
        "INSERT INTO account (name, password, algorithm) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(&account.name)
    .bind(&account.password)
    .bind(&account.algorithm)
    .fetch_one(conn)
    .await?)
}

/// Updates the password of an account.
pub async fn update_password(
    conn: &mut PgConnection,
    name: &str,
    password: &str,
    algorithm: PasswordHashAlgorithm,
) -> Result<()> {
    sqlx::query("UPDATE account SET password = $1, algorithm = $2 WHERE name = $3")
        .bind(password)
        .bind(algorithm)
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

/// Finds an account by id.
pub async fn get_by_id(conn: &mut PgConnection, id: i64) -> Result<Account> {
    Ok(
        sqlx::query_as::<_, Account>("SELECT * FROM account WHERE id = $1")
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
}

/// Finds an account by name.
pub async fn get_by_name(conn: &mut PgConnection, name: &str) -> Result<Account> {
    Ok(
        sqlx::query_as::<_, Account>("SELECT * FROM account WHERE name = $1")
            .bind(name)
            .fetch_one(conn)
            .await?,
    )
}

/// Deletes an account with the given id.
pub async fn delete_by_id(conn: &mut PgConnection, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM account WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Deletes an account with the given name.
pub async fn delete_by_name(conn: &mut PgConnection, name: &str) -> Result<()> {
    sqlx::query("DELETE FROM account WHERE name = $1")
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use chrono::prelude::*;
    use sqlx::PgPool;

    use crate::model::entity::Account;
    use crate::model::tests::db_test;
    use crate::model::PasswordHashAlgorithm;
    use crate::Result;

    use super::*;

    #[test]
    fn test_create_account() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let org_account = Account {
                id: -1,
                name: "testuser".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            };

            let db_account = create(&mut conn, &org_account).await?;

            assert_ne!(org_account.id, db_account.id);
            assert_eq!(org_account.name, db_account.name);
            assert_eq!(org_account.password, db_account.password);
            assert_eq!(org_account.algorithm, db_account.algorithm);
            assert_ne!(org_account.created_at, db_account.created_at);
            assert_ne!(org_account.updated_at, db_account.updated_at);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_update_password() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let old_password = "password1".to_string();
            let new_password = "password2".to_string();

            let org_account = Account {
                id: -1,
                name: "testuser".to_string(),
                password: old_password.clone(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            };

            let db_account = create(&mut conn, &org_account).await?;

            update_password(
                &mut conn,
                &org_account.name,
                &new_password,
                PasswordHashAlgorithm::Argon2,
            )
            .await?;

            let updated_db_account = get_by_id(&mut conn, db_account.id).await?;

            assert_eq!(updated_db_account.id, db_account.id);
            assert_eq!(updated_db_account.name, db_account.name);
            assert_ne!(updated_db_account.password, old_password);
            assert_eq!(updated_db_account.password, new_password);
            assert_eq!(updated_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_get_by_id() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create(&mut conn, &org_account).await?;
            }

            let get_db_account = get_by_id(&mut conn, 5).await?;

            assert_eq!(get_db_account.id, 5);
            assert_eq!(get_db_account.name, "testuser-5");
            assert_eq!(get_db_account.password, "testpassword-5");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_get_by_name() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create(&mut conn, &org_account).await?;
            }

            let get_db_account = get_by_name(&mut conn, "testuser-2").await?;

            assert_eq!(get_db_account.id, 2);
            assert_eq!(get_db_account.name, "testuser-2");
            assert_eq!(get_db_account.password, "testpassword-2");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_delete_by_id() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create(&mut conn, &org_account).await?;
            }

            delete_by_id(&mut conn, 5).await?;
            if let Err(e) = get_by_id(&mut conn, 5).await {
                match e.downcast_ref::<sqlx::Error>() {
                    Some(sqlx::Error::RowNotFound) => Ok(()),
                    Some(..) => Err(e),
                    None => Err(e),
                }?
            } else {
                panic!("record was not deleted");
            }
            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_delete_by_name() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create(&mut conn, &org_account).await?;
            }

            delete_by_name(&mut conn, "testuser-5").await?;
            if let Err(e) = get_by_id(&mut conn, 5).await {
                match e.downcast_ref::<sqlx::Error>() {
                    Some(sqlx::Error::RowNotFound) => Ok(()),
                    Some(..) => Err(e),
                    None => Err(e),
                }?
            } else {
                panic!("record was not deleted");
            }
            Ok(())
        }
        db_test(test)
    }
}
