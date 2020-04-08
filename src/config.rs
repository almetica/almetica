/// Module for the configuration handling.
use std::fs::File;
use std::path::PathBuf;

use serde::Deserialize;

use crate::*;

#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub database: DatabaseConfiguration,
    pub data: DataConfiguration,
    pub game: GameConfiguration,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfiguration {
    pub hostname: String,
    #[serde(alias = "web-port")]
    pub web_port: u16,
    #[serde(alias = "game-port")]
    pub game_port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfiguration {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Deserialize)]
pub struct DataConfiguration {
    pub path: PathBuf,
    pub key: String,
    pub iv: String,
}

#[derive(Debug, Deserialize)]
pub struct GameConfiguration {
    pub pvp: bool,
}

pub fn read_configuration(path: &PathBuf) -> Result<Configuration> {
    let f = File::open(path)?;
    let configuration = serde_yaml::from_reader(f)?;
    Ok(configuration)
}
