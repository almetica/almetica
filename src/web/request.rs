use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Login {
    pub username: String,
    pub password: String,
}
