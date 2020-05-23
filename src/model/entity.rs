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
#[derive(Clone, Debug, sqlx::FromRow)]
#[sqlx(rename = "account")]
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
#[derive(Clone, Debug, sqlx::FromRow)]
#[sqlx(rename = "login_ticket")]
#[sqlx(rename_all = "lowercase")]
pub struct LoginTicket {
    pub account_id: i64,
    pub ticket: Vec<u8>,
    pub used: bool,
    pub created_at: DateTime<Utc>,
}

// An account user. TERA calls a character an user.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub account_id: i64,
    pub name: String,
    pub gender: Gender,
    pub race: Race,
    pub class: Class,
    pub shape: Vec<u8>,
    pub details: Vec<u8>,
    pub appearance: Customization,
    pub appearance2: i32,
    pub level: i32,
    pub awakening_level: i32,
    pub laurel: i32,
    pub achievement_points: i32,
    pub playtime: i64, // Playtime in seconds.
    pub rest_bonus_xp: i64,
    pub show_face: bool,
    pub show_style: bool,
    pub lobby_slot: i32,
    pub is_new_character: bool,
    pub tutorial_state: i32,
    pub is_deleting: bool,
    pub delete_at: Option<DateTime<Utc>>,
    pub last_logout_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
