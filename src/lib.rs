#![warn(clippy::all)]
#![recursion_limit = "256"]
pub mod config;
pub mod crypt;
pub mod dataloader;
pub mod ecs;
pub mod model;
pub mod networkserver;
pub mod protocol;
pub mod webserver;
use thiserror::Error;

pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

/// Custom errors. Only create a custom error if you re-use them on different
/// places or are using them to catch special error conditions (by checking for
/// them). Try to use ```anyhow::bail!``` and ```anyhow::Context``` for error
/// handling if possible.
#[derive(Error, Debug)]
pub enum AlmeticaError {
    #[error("connection closed2")]
    ConnectionClosed,

    #[error("no message mapping found for packet")]
    NoMessageMappingForPacket,

    #[error("client sent authenticated packet without being authenticated")]
    UnauthorizedPacket,

    #[error("unsupported password hash")]
    UnsupportedPasswordHash,

    #[error("invalid login provided")]
    InvalidLogin,
}
