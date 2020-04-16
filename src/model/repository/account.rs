/// Handles the accounts of the player.
use postgres::GenericClient;

use crate::model::entity::Account;
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

/// Updates an account.
pub fn update<C>(_conn: &mut C) -> Result<()>
where
    C: GenericClient,
{
    Ok(())
}

/// Finds an account by ID.
pub fn get_by_id<C>(_conn: &mut C, _id: u64) -> Result<()>
where
    C: GenericClient,
{
    Ok(())
}

/// Finds an account by name.
pub fn get_by_name<C>(_conn: &mut C, _name: &str) -> Result<()>
where
    C: GenericClient,
{
    Ok(())
}

/// Deletes a account with the given ID.
pub fn delete<C>(_conn: &mut C, _id: u64) -> Result<()>
where
    C: GenericClient,
{
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
}
