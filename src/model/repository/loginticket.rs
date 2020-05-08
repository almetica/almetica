/// Handles the login ticket of client connections.
use crate::model::entity::LoginTicket;
use crate::Result;
use rand::rngs::OsRng;
use rand::RngCore;
use sqlx::prelude::*;
use sqlx::PgConnection;

/// Upserts a ticket (randomly generated 128 bytes). Tickets are valid for 5 minutes and can only be used once.
pub async fn upsert_ticket(conn: &mut PgConnection, account_id: i64) -> Result<LoginTicket> {
    let mut ticket = vec![0u8; 128];
    OsRng.fill_bytes(&mut ticket);

    Ok(sqlx::query_as::<_, LoginTicket>(
        r#"INSERT INTO login_ticket VALUES ($1, $2, DEFAULT, DEFAULT)
        ON CONFLICT (account_id) DO UPDATE SET ticket = $2, used = DEFAULT, created_at = DEFAULT
        RETURNING *"#,
    )
    .bind(account_id)
    .bind(ticket)
    .fetch_one(conn)
    .await?)
}

/// Tests if the given ticket is valid. A ticket can only be used one time. Should be called in a transaction.
pub async fn is_ticket_valid(conn: &mut PgConnection, name: &str, ticket: &[u8]) -> Result<bool> {
    // We have to manually re-borrow the transaction. &mut *conn will take a &mut PgConnection and
    // produce a &mut PgConnection that is held for the lifetime required by the function.
    // This is normally done implicitly by Rust. It's not in this case due to fetch_*() being
    // generic over its parameter (allowing both connection, a pool or a transaction to be passed in).

    let account_id: i64 = match sqlx::query_as(
        r#" 
               SELECT l.account_id
               FROM login_ticket l
               INNER JOIN account a
               ON l.account_id = a.id
               WHERE a.name = $1
               AND l.ticket = $2
               AND l.used = 'false'
               AND age(CURRENT_TIMESTAMP, l.created_at) < INTERVAL '5 minutes'"#,
    )
    .bind(name)
    .bind(ticket)
    .fetch_optional(&mut *conn)
    .await?
    {
        Some((id,)) => id,
        None => return Ok(false),
    };

    sqlx::query("UPDATE login_ticket SET used = 'true' WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *conn)
        .await?;

    Ok(true)
}

#[cfg(test)]
pub mod tests {
    use chrono::prelude::*;
    use sqlx::PgPool;

    use crate::model::entity::Account;
    use crate::model::repository::account;
    use crate::model::tests::db_test;
    use crate::model::PasswordHashAlgorithm;
    use crate::Result;

    use super::*;

    #[test]
    fn test_upsert_login_ticket() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let account = account::create(
                &mut conn,
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

            let first_ticket = upsert_ticket(&mut conn, account.id).await?;
            let second_ticket = upsert_ticket(&mut conn, account.id).await?;

            assert_eq!(first_ticket.account_id, second_ticket.account_id);
            assert_ne!(first_ticket.ticket, second_ticket.ticket);
            assert_ne!(first_ticket.created_at, second_ticket.created_at);

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_validate_valid_ticket() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let account = account::create(
                &mut conn,
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

            let ticket = upsert_ticket(&mut conn, account.id).await?;
            assert!(!ticket.ticket.is_empty());
            assert!(is_ticket_valid(&mut conn, &account.name, &ticket.ticket).await?);
            // Ticket can only be used one time
            assert!(!is_ticket_valid(&mut conn, &account.name, &ticket.ticket).await?);

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_validate_invalid_ticket_1() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let account = account::create(
                &mut conn,
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

            upsert_ticket(&mut conn, account.id).await?;
            assert!(!is_ticket_valid(&mut conn, &account.name, "123456789".as_bytes()).await?);

            Ok(())
        }
        db_test(test)
    }

    #[test]
    fn test_validate_invalid_ticket_2() -> Result<()> {
        // FIXME into an async closure once stable
        async fn test(pool: PgPool) -> Result<()> {
            let mut conn = pool.acquire().await.unwrap();

            let account = account::create(
                &mut conn,
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

            let ticket = upsert_ticket(&mut conn, account.id).await?;
            assert!(!is_ticket_valid(&mut conn, &"not-a-user", &ticket.ticket).await?);

            Ok(())
        }
        db_test(test)
    }
}
