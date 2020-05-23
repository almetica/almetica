/// Module that holds all systems used by the ECS.
use crate::ecs::message::EcsMessage;
use async_std::sync::{Sender, TrySendError};
use tracing::{debug, trace};

// TODO we could think about including the debug!("XXX incoming") too
#[macro_export]
#[allow(unused_macros)]
macro_rules! id_span {
    ($v:ident) => (
        let span = info_span!("id", $v = ?$v);
        let _enter = span.enter();
    );
}

pub mod common;
pub mod global;
pub mod local;

/// Send a message using the given channel.
pub fn send_message(message: EcsMessage, channel: &Sender<EcsMessage>) {
    debug!("Sending outgoing {}", message);
    trace!("Message data: {:?}", message);
    match channel.try_send(message) {
        Ok(..) => {}
        Err(TrySendError::Full(..)) => {
            debug!("Dropping message for connection because channel is full")
        }
        Err(TrySendError::Disconnected(..)) => {
            debug!("Dropping message for connection because channel is disconnected")
        }
    }
}
