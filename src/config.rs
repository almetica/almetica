/// Module for the configuration handling.
use std::fs::File;
use std::path::PathBuf;

use super::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Configuration {
    pub data: DataConfiguration,
}

#[derive(Debug, Deserialize)]
pub struct DataConfiguration {
    pub path: PathBuf,
    pub key: String,
    pub iv: String,
}

pub fn read_configuration(path: &PathBuf) -> Result<Configuration> {
    let f = File::open(path)?;
    let configuration = serde_yaml::from_reader(f)?;
    Ok(configuration)
}
