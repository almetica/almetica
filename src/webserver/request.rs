use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Login {
    pub accountname: String,
    pub password: String,
}
