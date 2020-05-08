/// Holds all database entities.
///
/// Supported data types:
///    * ```i8```, ```i16```, ```i32```, ```i64```
///    * ```String```
///    * ```Vec```
///    * ```Bool```
///    * ```Customization```
///    * Custom types / ```enum``` based on the above.
///    * DateTime<Utc>
use crate::model::*;
use chrono::{DateTime, Utc};

/// Account that holds the login information of a player.
#[derive(Debug, sqlx::FromRow)]
#[sqlx(rename_all = "lowercase")]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub password: String,
    pub algorithm: PasswordHashAlgorithm,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Ticket that is used to authenticate the client connection.
#[derive(Debug, sqlx::FromRow)]
#[sqlx(rename_all = "lowercase")]
pub struct LoginTicket {
    pub account_id: i64,
    pub ticket: Vec<u8>,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
#[sqlx(rename = "account_user")]
#[sqlx(rename_all = "lowercase")]
pub struct User {
    pub id: i32,
    pub account_id: i64,
    pub name: String,
    pub gender: Gender,
    pub race: Race,
    #[sqlx(rename = "user_class")]
    pub class: Class,
    pub shape: Vec<u8>,
    pub details: Vec<u8>,
    pub appearance: Customization,
    pub appearance2: i32,
    pub playtime: i64, // Playtime in seconds.
    pub created_at: DateTime<Utc>,
}
