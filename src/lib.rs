#![warn(clippy::all)]

use std::sync::Arc;

use thiserror::Error;

use ecs::event::Event;

pub mod config;
pub mod crypt;
pub mod dataloader;
pub mod ecs;
pub mod gameserver;
pub mod model;
pub mod protocol;
pub mod webserver;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("magic word not found at start of the stream")]
    NoMagicWord,

    #[error("connection closed")]
    ConnectionClosed,

    #[error("no event mapping found for packet")]
    NoEventMappingForPacket,

    #[error("no sender open for response channel")]
    NoSenderResponseChannel,

    #[error("no sender open when waiting for connection entity")]
    NoSenderWaitingConnectionEntity,

    #[error("entity was not set")]
    EntityNotSet,

    #[error("KEY or IV have the wrong size. Needs to be 128 bit")]
    KeyOrIvWrongSize,

    #[error("decompression was successful, but data was missing to finish it")]
    DecompressionNotFinished,

    #[error("wrong event received")]
    WrongEventReceived,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_yaml::Error),

    #[error("protocol serde error: {0}")]
    ProtocolSerde(#[from] protocol::serde::Error),

    #[error("mpsc send event error: {0}")]
    MpscSendEvent(#[from] tokio::sync::mpsc::error::SendError<Arc<Event>>),

    #[error("tokio timeout error: {0}")]
    TokioTimeOut(#[from] tokio::time::Elapsed),

    #[error("tokio join error: {0}")]
    TokioJoinError(#[from] tokio::task::JoinError),

    #[error("utf8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("hex error: {0}")]
    FromHex(#[from] hex::FromHexError),

    #[error("flate2 decompress error: {0}")]
    Flate2Decompress(#[from] flate2::DecompressError),

    #[error("unknown error")]
    Unknown,
}
