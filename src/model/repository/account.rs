/// Handles the accounts of the player.
use crate::model::entity::Account;
use crate::model::PasswordHashAlgorithm;
use crate::{Error, Result};

/// Creates an new account.
pub fn create<C>(conn: &mut C, account: &Account) -> Result<Account>
where
    C: postgres::GenericClient,
{
    match conn.query_opt(
        "INSERT INTO account (name, password, algorithm) VALUES ($1, $2, $3) RETURNING (account)",
        &[&account.name, &account.password, &account.algorithm],
    )? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Creates an new account.
pub async fn create_async<C>(conn: &mut C, account: &Account) -> Result<Account>
where
    C: tokio_postgres::GenericClient,
{
    match conn.query_opt(
        "INSERT INTO account (name, password, algorithm) VALUES ($1, $2, $3) RETURNING (account)",
        &[&account.name, &account.password, &account.algorithm],
    ).await? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Updates the password of an account.
pub async fn update_password_async<C>(
    conn: &mut C,
    name: &str,
    password: &str,
    algorithm: PasswordHashAlgorithm,
) -> Result<(), Error>
where
    C: tokio_postgres::GenericClient,
{
    conn.execute(
        "UPDATE account SET password = $1, algorithm = $2 WHERE name = $3",
        &[&password, &algorithm, &name],
    )
    .await?;
    Ok(())
}

/// Finds an account by id.
pub async fn get_by_id_async<C>(conn: &mut C, id: i64) -> Result<Account>
where
    C: tokio_postgres::GenericClient,
{
    match conn
        .query_opt("SELECT (account) FROM account WHERE id = $1", &[&id])
        .await?
    {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Finds an account by name.
pub async fn get_by_name_async<C>(conn: &mut C, name: &str) -> Result<Account>
where
    C: tokio_postgres::GenericClient,
{
    match conn
        .query_opt("SELECT (account) FROM account WHERE name = $1", &[&name])
        .await?
    {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Finds an account by name.
pub fn get_by_name<C>(conn: &mut C, name: &str) -> Result<Account>
where
    C: postgres::GenericClient,
{
    match conn.query_opt("SELECT (account) FROM account WHERE name = $1", &[&name])? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Deletes an account with the given id.
pub async fn delete_by_id_async<C>(conn: &mut C, id: i64) -> Result<()>
where
    C: tokio_postgres::GenericClient,
{
    conn.execute("DELETE FROM account WHERE id = $1", &[&id])
        .await?;
    Ok(())
}

/// Deletes an account with the given name.
pub async fn delete_by_name_async<C>(conn: &mut C, name: &str) -> Result<()>
where
    C: tokio_postgres::GenericClient,
{
    conn.execute("DELETE FROM account WHERE name = $1", &[&name])
        .await?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use chrono::prelude::*;

    use crate::model::entity::Account;
    use crate::model::tests::{async_db_test, db_test};
    use crate::model::PasswordHashAlgorithm;
    use crate::{AsyncDbPool, Result};

    use super::*;

    #[test]
    fn test_create_account() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let org_account = Account {
                id: -1,
                name: "testuser".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            };

            let conn = &mut *db_pool.get().await?;
            let db_account = create_async(conn, &org_account).await?;

            assert_ne!(org_account.id, db_account.id);
            assert_eq!(org_account.name, db_account.name);
            assert_eq!(org_account.password, db_account.password);
            assert_eq!(org_account.algorithm, db_account.algorithm);
            assert_ne!(org_account.created_at, db_account.created_at);
            assert_ne!(org_account.updated_at, db_account.updated_at);
            Ok(())
        }
        async_db_test(test)
    }

    #[test]
    fn test_update_password() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
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

            let conn = &mut *db_pool.get().await?;
            let db_account = create_async(conn, &org_account).await?;

            update_password_async(
                conn,
                &org_account.name,
                &new_password,
                PasswordHashAlgorithm::Argon2,
            )
            .await?;

            let updated_db_account = get_by_id_async(conn, db_account.id).await?;

            assert_eq!(updated_db_account.id, db_account.id);
            assert_eq!(updated_db_account.name, db_account.name);
            assert_ne!(updated_db_account.password, old_password);
            assert_eq!(updated_db_account.password, new_password);
            assert_eq!(updated_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        async_db_test(test)
    }

    #[test]
    fn test_get_by_id() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let conn = &mut *db_pool.get().await?;

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create_async(conn, &org_account).await?;
            }

            let get_db_account = get_by_id_async(conn, 5).await?;

            assert_eq!(get_db_account.id, 5);
            assert_eq!(get_db_account.name, "testuser-5");
            assert_eq!(get_db_account.password, "testpassword-5");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        async_db_test(test)
    }

    #[test]
    fn test_get_by_name_sync() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create(conn, &org_account)?;
            }

            let get_db_account = get_by_name(conn, "testuser-2")?;

            assert_eq!(get_db_account.id, 2);
            assert_eq!(get_db_account.name, "testuser-2");
            assert_eq!(get_db_account.password, "testpassword-2");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        })
    }

    #[test]
    fn test_get_by_name_async() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let conn = &mut *db_pool.get().await?;

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create_async(conn, &org_account).await?;
            }

            let get_db_account = get_by_name_async(conn, "testuser-2").await?;

            assert_eq!(get_db_account.id, 2);
            assert_eq!(get_db_account.name, "testuser-2");
            assert_eq!(get_db_account.password, "testpassword-2");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        }
        async_db_test(test)
    }

    #[test]
    fn test_delete_by_id() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let conn = &mut *db_pool.get().await?;

            for i in 1..=10 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create_async(conn, &org_account).await?;
            }

            delete_by_id_async(conn, 5).await?;
            let res = get_by_id_async(conn, 5).await;
            match res {
                Err(Error::NoRowReturned) => Ok(()),
                Err(e) => Err(e),
                _ => panic!("record was not deleted"),
            }
        }
        async_db_test(test)
    }

    #[test]
    fn test_delete_by_name() -> Result<()> {
        // FIXME into into an async closure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let conn = &mut *db_pool.get().await?;

            for i in 1..=10i32 {
                let org_account = Account {
                    id: -1,
                    name: format!("testuser-{}", i),
                    password: format!("testpassword-{}", i),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                };
                create_async(conn, &org_account).await?;
            }

            delete_by_name_async(conn, "testuser-5").await?;
            let res = get_by_name_async(conn, "testuser-5").await;
            match res {
                Err(Error::NoRowReturned) => Ok(()),
                Err(e) => Err(e),
                _ => panic!("record was not deleted"),
            }
        }
        async_db_test(test)
    }
}
