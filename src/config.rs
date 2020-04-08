/// Module for the configuration handling.
use std::fs::File;
use std::path::PathBuf;

use crate::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub data: DataConfiguration,
    pub game: GameConfiguration
}

#[derive(Debug, Deserialize)]
pub struct ServerConfiguration {
    pub hostname: String,
    pub web_port: u16,
    pub game_port: u16,
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
