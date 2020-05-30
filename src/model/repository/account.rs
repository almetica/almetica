/// Handles the accounts of the player.
use crate::model::entity::Account;
use crate::model::PasswordHashAlgorithm;
use crate::Result;
use sqlx::prelude::*;
use sqlx::PgConnection;

/// Creates a new account.
pub async fn create(conn: &mut PgConnection, account: &Account) -> Result<Account> {
    Ok(sqlx::query_as::<_, Account>(
        r#"INSERT INTO "account" ("name", "password", "algorithm") VALUES ($1, $2, $3) RETURNING *"#,
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
    sqlx::query(r#"UPDATE "account" SET "password" = $1, "algorithm" = $2 WHERE "name" = $3"#)
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
        sqlx::query_as::<_, Account>(r#"SELECT * FROM "account" WHERE "id" = $1"#)
            .bind(id)
            .fetch_one(conn)
            .await?,
    )
}

/// Finds an account by name.
pub async fn get_by_name(conn: &mut PgConnection, name: &str) -> Result<Account> {
    Ok(
        sqlx::query_as::<_, Account>(r#"SELECT * FROM "account" WHERE "name" = $1"#)
            .bind(name)
            .fetch_one(conn)
            .await?,
    )
}

/// Deletes an account with the given id.
pub async fn delete_by_id(conn: &mut PgConnection, id: i64) -> Result<()> {
    sqlx::query(r#"DELETE FROM "account" WHERE "id" = $1"#)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Deletes an account with the given name.
pub async fn delete_by_name(conn: &mut PgConnection, name: &str) -> Result<()> {
    sqlx::query(r#"DELETE FROM "account" WHERE "name" = $1"#)
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::entity::Account;
    use crate::model::tests::db_test;
    use crate::model::PasswordHashAlgorithm;
    use crate::Result;
    use async_std::task;
    use chrono::prelude::*;
    use sqlx::PgConnection;

    pub fn get_default_account(num: i32) -> Account {
        Account {
            id: -1,
            name: format!("testaccount-{}", num),
            password: format!("testpassword-{}", num),
            algorithm: PasswordHashAlgorithm::Argon2,
            created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
        }
    }

    #[test]
    fn test_create_account() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;
                let org_account = get_default_account(0);
                let db_account = create(&mut conn, &org_account).await?;

                assert_ne!(org_account.id, db_account.id);
                assert_eq!(org_account.name, db_account.name);
                assert_eq!(org_account.password, db_account.password);
                assert_eq!(org_account.algorithm, db_account.algorithm);
                assert_ne!(org_account.created_at, db_account.created_at);
                assert_ne!(org_account.updated_at, db_account.updated_at);

                Ok(())
            })
        })
    }

    #[test]
    fn test_update_password() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;

                let old_password = "password1".to_string();
                let new_password = "password2".to_string();
                let org_account = get_default_account(0);
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
            })
        })
    }

    #[test]
    fn test_get_by_id() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;

                for i in 1..=10i32 {
                    create(&mut conn, &get_default_account(i)).await?;
                }

                let get_db_account = get_by_id(&mut conn, 5).await?;

                assert_eq!(get_db_account.id, 5);
                assert_eq!(get_db_account.name, "testaccount-5");
                assert_eq!(get_db_account.password, "testpassword-5");
                assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);

                Ok(())
            })
        })
    }

    #[test]
    fn test_get_by_name() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;

                for i in 1..=10i32 {
                    create(&mut conn, &get_default_account(i)).await?;
                }

                let get_db_account = get_by_name(&mut conn, "testaccount-2").await?;

                assert_eq!(get_db_account.id, 2);
                assert_eq!(get_db_account.name, "testaccount-2");
                assert_eq!(get_db_account.password, "testpassword-2");
                assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);

                Ok(())
            })
        })
    }

    #[test]
    fn test_delete_by_id() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;

                for i in 1..=10i32 {
                    let org_account = get_default_account(i);
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
                    panic!("Record was not deleted");
                }
                Ok(())
            })
        })
    }

    #[test]
    fn test_delete_by_name() -> Result<()> {
        db_test(|db_string| {
            task::block_on(async {
                let mut conn = PgConnection::connect(db_string).await?;

                for i in 1..=10i32 {
                    create(&mut conn, &get_default_account(i)).await?;
                }

                delete_by_name(&mut conn, "testaccount-5").await?;
                if let Err(e) = get_by_id(&mut conn, 5).await {
                    match e.downcast_ref::<sqlx::Error>() {
                        Some(sqlx::Error::RowNotFound) => Ok(()),
                        Some(..) => Err(e),
                        None => Err(e),
                    }?
                } else {
                    panic!("Record was not deleted");
                }

                Ok(())
            })
        })
    }
}
