/// Module for the configuration handling.
use crate::*;
use serde::Deserialize;
use std::fs::File;
use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize)]
pub struct Configuration {
    pub server: ServerConfiguration,
    pub database: DatabaseConfiguration,
    pub data: DataConfiguration,
    pub game: GameConfiguration,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfiguration {
    pub ip: Ipv4Addr,
    #[serde(alias = "web-port")]
    pub web_port: u16,
    #[serde(alias = "game-port")]
    pub game_port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfiguration {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DataConfiguration {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GameConfiguration {
    pub pvp: bool,
}

pub fn read_configuration(path: &PathBuf) -> Result<Configuration> {
    let f = File::open(path)?;
    let configuration = serde_yaml::from_reader(f)?;
    Ok(configuration)
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            server: ServerConfiguration {
                ip: Ipv4Addr::new(127, 0, 0, 1),
                web_port: 0,
                game_port: 0,
            },
            database: DatabaseConfiguration {
                hostname: "".to_string(),
                port: 0,
                username: "".to_string(),
                password: "".to_string(),
                database: "".to_string(),
            },
            data: DataConfiguration {
                path: Default::default(),
            },
            game: GameConfiguration { pvp: false },
        }
    }
}
