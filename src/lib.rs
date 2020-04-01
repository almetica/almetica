#![warn(clippy::all)]
pub mod config;
pub mod crypt;
pub mod dataloader;
pub mod ecs;
pub mod model;
pub mod protocol;

use ecs::event::Event;
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("magic word not found at start of the stream")]
    NoMagicWord,

    #[error("no event mapping found for packet")]
    NoEventMappingForPacket,

    #[error("no sender open for response channel")]
    NoSenderResponseChannel,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_yaml::Error),

    #[error("protocol serde error: {0}")]
    ProtocolSerde(#[from] protocol::serde::Error),

    #[error("mpsc send event error: {0}")]
    MpscSendEventError(#[from] tokio::sync::mpsc::error::SendError<Box<Event>>),

    #[error("unknown error")]
    Unknown,
}
