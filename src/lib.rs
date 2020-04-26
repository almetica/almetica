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

use std::sync::Arc;

use thiserror::Error;

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

    #[error("unsupported password hash")]
    UnsupportedPasswordHash,

    #[error("parse int error")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("argon2 error: {0}")]
    Argon2(#[from] argon2::Error),

    #[error("future timeout error: {0}")]
    FutureTimeout(#[from] async_std::future::TimeoutError),

    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("dotenv error: {0}")]
    Dotenv(#[from] dotenv::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_yaml::Error),

    #[error("protocol serde error: {0}")]
    ProtocolSerde(#[from] protocol::serde::Error),

    #[error("refinery error: {0}")]
    Refinery(#[from] refinery::Error),

    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("tokio timeout error: {0}")]
    TokioTimeOut(#[from] tokio::time::Elapsed),

    #[error("tokio progres error: {0}")]
    TokioProgresError(#[from] tokio_postgres::error::Error),

    #[error("utf8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("hex error: {0}")]
    FromHex(#[from] hex::FromHexError),

    #[error("flate2 decompress error: {0}")]
    Flate2Decompress(#[from] flate2::DecompressError),

    #[error("unknown error")]
    Unknown,
}
