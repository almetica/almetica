/// Handles the login ticket of client connections.
use postgres;

use crate::model::entity::LoginTicket;
use crate::{Error, Result};
use hex::encode;
use rand::rngs::OsRng;
use rand::RngCore;

/// Upserts a ticket (randomly generated 64 bytes encoded as hex). Tickets are valid for 5 minutes and can only be used once.
pub fn upsert_ticket<C>(conn: &mut C, account_id: i64) -> Result<LoginTicket>
where
    C: postgres::GenericClient,
{
    let mut ticket = vec![0u8; 64];
    OsRng.fill_bytes(&mut ticket);

    match conn.query_opt(
        r#"INSERT INTO login_ticket VALUES ($1, $2, DEFAULT, DEFAULT)
        ON CONFLICT (account_id) DO UPDATE SET ticket = $2, used = DEFAULT, created_at = DEFAULT
        RETURNING (login_ticket)"#,
        &[&account_id, &encode(ticket)],
    )? {
        Some(row) => Ok(row.get::<usize, LoginTicket>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Upserts a ticket (randomly generated 64 bytes encoded as hex). Tickets are valid for 5 minutes and can only be used once.
pub async fn upsert_ticket_async<C>(conn: &mut C, account_id: i64) -> Result<LoginTicket>
where
    C: tokio_postgres::GenericClient,
{
    let mut ticket = vec![0u8; 64];
    OsRng.fill_bytes(&mut ticket);

    match conn
        .query_opt(
            r#"INSERT INTO login_ticket VALUES ($1, $2, DEFAULT, DEFAULT)
        ON CONFLICT (account_id) DO UPDATE SET ticket = $2, used = DEFAULT, created_at = DEFAULT
        RETURNING (login_ticket)"#,
            &[&account_id, &encode(ticket)],
        )
        .await?
    {
        Some(row) => Ok(row.get::<usize, LoginTicket>(0)),
        None => Err(Error::NoRowReturned),
    }
}

/// Tests if the given ticket is valid. A ticket can only be used one time. Should be called in a transaction.
pub fn is_ticket_valid<C>(conn: &mut C, account_id: i64, ticket: &str) -> Result<bool>
where
    C: postgres::GenericClient,
{
    let found = match conn.query_opt(
        r#"
        SELECT EXISTS( 
            SELECT account_id FROM login_ticket 
            WHERE account_id = $1 AND ticket = $2 AND used = 'false' AND age(CURRENT_TIMESTAMP, created_at) < INTERVAL '5 minutes'
        )"#,
        &[&account_id, &ticket],
    )? {
        Some(row) => row.get::<usize, bool>(0),
        None => return Err(Error::NoRowReturned),
    };
    if found {
        conn.execute(
            "UPDATE login_ticket SET used = 'true' WHERE account_id = $1",
            &[&account_id],
        )?;
    }
    Ok(found)
}

#[cfg(test)]
pub mod tests {
    use chrono::prelude::*;

    use crate::model::entity::Account;
    use crate::model::repository::account;
    use crate::model::tests::{async_db_test, db_test};
    use crate::model::PasswordHashAlgorithm;
    use crate::{AsyncDbPool, Result};

    use super::*;

    #[test]
    fn test_upsert_login_ticket() -> Result<()> {
        // FIXME into an async clojure once stable
        async fn test(db_pool: AsyncDbPool) -> Result<()> {
            let conn = &mut *db_pool.get().await?;
            let account = account::create_async(
                conn,
                &Account {
                    id: -1,
                    name: "testuser".to_string(),
                    password: "not-a-real-password-hash".to_string(),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                },
            )
            .await?;

            let first_ticket = upsert_ticket_async(conn, account.id).await?;
            let second_ticket = upsert_ticket_async(conn, account.id).await?;

            assert_eq!(first_ticket.account_id, second_ticket.account_id);
            assert_ne!(first_ticket.ticket, second_ticket.ticket);
            assert_ne!(first_ticket.created_at, second_ticket.created_at);

            Ok(())
        }
        async_db_test(test)
    }

    #[test]
    fn test_validate_valid_ticket() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;

            let account = account::create(
                conn,
                &Account {
                    id: -1,
                    name: "testuser".to_string(),
                    password: "not-a-real-password-hash".to_string(),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                },
            )?;

            let ticket = upsert_ticket(conn, account.id)?;
            assert!(!ticket.ticket.is_empty());
            assert!(is_ticket_valid(conn, account.id, &ticket.ticket)?);
            // Ticket can only be used one time
            assert!(!is_ticket_valid(conn, account.id, &ticket.ticket)?);

            Ok(())
        })
    }

    #[test]
    fn test_validate_invalid_ticket_1() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;
            let account = account::create(
                conn,
                &Account {
                    id: -1,
                    name: "testuser".to_string(),
                    password: "not-a-real-password-hash".to_string(),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                },
            )?;

            upsert_ticket(conn, account.id)?;
            assert!(!is_ticket_valid(conn, account.id, "not a valid ticket")?);

            Ok(())
        })
    }

    #[test]
    fn test_validate_invalid_ticket_2() -> Result<()> {
        db_test(|db_pool| {
            let conn = &mut *db_pool.get()?;
            let account = account::create(
                conn,
                &Account {
                    id: -1,
                    name: "testuser".to_string(),
                    password: "not-a-real-password-hash".to_string(),
                    algorithm: PasswordHashAlgorithm::Argon2,
                    created_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                    updated_at: Utc.ymd(1995, 7, 8).and_hms(9, 10, 11),
                },
            )?;

            let ticket = upsert_ticket(conn, account.id)?;
            assert!(!is_ticket_valid(conn, 100, &ticket.ticket)?);

            Ok(())
        })
    }
}
