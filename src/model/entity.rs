/// Holds all database entities.
use chrono::{DateTime, Utc};
use postgres_types::{FromSql, ToSql};

use crate::model::PasswordHashAlgorithm;

/// Account that holds the login information of a player.
#[derive(Debug, FromSql, ToSql)]
#[postgres(name = "account")]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub password: String,
    pub algorithm: PasswordHashAlgorithm,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Ticket that is used to authenticate the client connection.
#[derive(Debug, FromSql, ToSql)]
#[postgres(name = "login_ticket")]
pub struct LoginTicket {
    pub account_id: i64,
    pub ticket: String,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}
