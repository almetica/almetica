/// Handles the accounts of the player.
use postgres::GenericClient;

use crate::model::entity::Account;
use crate::model::PasswordHashAlgorithm;
use crate::{Error, Result};

/// Creates an new account.
pub fn create<C>(conn: &mut C, account: &Account) -> Result<Account>
where
    C: GenericClient,
{
    match conn.query_opt(
        "INSERT INTO account (name, password, algorithm) VALUES ($1, $2, $3) RETURNING (account)",
        &[&account.name, &account.password, &account.algorithm],
    )? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Updates the password of an account.
pub fn update_password<C>(
    conn: &mut C,
    name: &str,
    password: &str,
    algorithm: PasswordHashAlgorithm,
) -> Result<(), Error>
where
    C: GenericClient,
{
    conn.execute(
        "UPDATE account SET password = $1, algorithm = $2 WHERE name = $3",
        &[&password, &algorithm, &name],
    )?;
    Ok(())
}

/// Finds an account by id.
pub fn get_by_id<C>(conn: &mut C, id: i64) -> Result<Account>
where
    C: GenericClient,
{
    match conn.query_opt("SELECT (account) FROM account WHERE id = $1", &[&id])? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Finds an account by name.
pub fn get_by_name<C>(conn: &mut C, name: &str) -> Result<Account>
where
    C: GenericClient,
{
    match conn.query_opt("SELECT (account) FROM account WHERE name = $1", &[&name])? {
        Some(row) => Ok(row.get::<usize, Account>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Deletes an account with the given id.
pub fn delete_by_id<C>(conn: &mut C, id: i64) -> Result<()>
where
    C: GenericClient,
{
    conn.execute("DELETE FROM account WHERE id = $1", &[&id])?;
    Ok(())
}

/// Deletes an account with the given name.
pub fn delete_by_name<C>(conn: &mut C, name: &str) -> Result<()>
where
    C: GenericClient,
{
    conn.execute("DELETE FROM account WHERE name = $1", &[&name])?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use chrono::prelude::*;

    use crate::model::entity::Account;
    use crate::model::tests::db_test;
    use crate::model::PasswordHashAlgorithm;
    use crate::Result;

    use super::*;

    #[test]
    fn test_create_account() -> Result<()> {
        db_test(|db_pool| {
            let org_account = Account {
                id: -1,
                name: "testuser".to_string(),
                password: "not-a-real-password-hash".to_string(),
                algorithm: PasswordHashAlgorithm::Argon2,
                created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
            };

            let conn = &mut *db_pool.get()?;
            let db_account = create(conn, &org_account)?;

            assert_ne!(org_account.id, db_account.id);
            assert_eq!(org_account.name, db_account.name);
            assert_eq!(org_account.password, db_account.password);
            assert_eq!(org_account.algorithm, db_account.algorithm);
            assert_ne!(org_account.created_at, db_account.created_at);
            assert_ne!(org_account.updated_at, db_account.updated_at);
            Ok(())
        })
    }

    #[test]
    fn test_update_password() -> Result<()> {
        db_test(|db_pool| {
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

            let conn = &mut *db_pool.get()?;
            let db_account = create(conn, &org_account)?;

            update_password(
                conn,
                &org_account.name,
                &new_password,
                PasswordHashAlgorithm::Argon2,
            )?;

            let updated_db_account = get_by_id(conn, db_account.id)?;

            assert_eq!(updated_db_account.id, db_account.id);
            assert_eq!(updated_db_account.name, db_account.name);
            assert_ne!(updated_db_account.password, old_password);
            assert_eq!(updated_db_account.password, new_password);
            assert_eq!(updated_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        })
    }

    #[test]
    fn test_get_by_id() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            for i in 1..=10 {
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

            let get_db_account = get_by_id(conn, 5)?;

            assert_eq!(get_db_account.id, 5);
            assert_eq!(get_db_account.name, "testuser-5");
            assert_eq!(get_db_account.password, "testpassword-5");
            assert_eq!(get_db_account.algorithm, PasswordHashAlgorithm::Argon2);
            Ok(())
        })
    }

    #[test]
    fn test_get_by_name() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            for i in 1..=10 {
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
    fn test_delete_by_id() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            for i in 1..=10 {
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

            delete_by_id(conn, 5)?;
            let res = get_by_id(conn, 5);
            match res {
                Err(Error::NoRowReturned) => Ok(()),
                Err(e) => Err(e),
                _ => panic!("record was not deleted"),
            }
        })
    }

    #[test]
    fn test_delete_by_name() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            for i in 1..=10 {
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

            delete_by_name(conn, "testuser-5")?;
            let res = get_by_name(conn, "testuser-5");
            match res {
                Err(Error::NoRowReturned) => Ok(()),
                Err(e) => Err(e),
                _ => panic!("record was not deleted"),
            }
        })
    }
}
