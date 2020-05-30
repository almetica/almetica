/// Holds the logic to interact with the database. A `conn` can either be a ```sqlx::PgConnection```
/// or a ```sqlx::Transaction``` by using ```&mut *tx```.
pub mod account;
pub mod loginticket;
pub mod user;
pub mod user_location;
