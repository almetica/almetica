/// Module that holds all systems used by the ECS.
mod cleaner;
mod connection_manager;
mod event_receiver;
mod settings_manager;
mod user_manager;

pub use cleaner::cleaner_system;
pub use connection_manager::connection_manager_system;
pub use event_receiver::event_receiver_system;
pub use settings_manager::settings_manager_system;
pub use user_manager::user_manager_system;

use crate::ecs::component::Connection;
use crate::ecs::event::EcsEvent;
use async_std::sync::TrySendError;
use tracing::{debug, error, info_span, trace};

/// Send an outgoing event.
pub fn send_event<'a, T>(event: EcsEvent, connections: T)
where
    T: shipyard::Get<Out = &'a Connection>,
{
    if let Some(connection_id) = event.connection_id() {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        if let Ok(connection) = connections.try_get(connection_id) {
            send(event, connection);
        } else {
            debug!("Couldn't find connection: {:?}", connection_id);
        }
    } else {
        error!("Event didn't had an connection attached");
    }
}

/// Send an outgoing event using the given connection.
pub fn send_event_with_connection(event: EcsEvent, connection: &Connection) {
    if let Some(connection_id) = event.connection_id() {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        debug!("Sending outgoing event {}", event);
        send(event, connection);
    } else {
        error!("Event didn't had an connection attached");
    }
}

fn send(event: EcsEvent, connection: &Connection) {
    debug!("Sending outgoing event {}", event);
    trace!("Event data: {:?}", event);
    match connection.channel.try_send(event) {
        Ok(..) => {}
        Err(TrySendError::Full(..)) => {
            debug!("Dropping event for connection because channel is full")
        }
        Err(TrySendError::Disconnected(..)) => {
            debug!("Dropping event because channel is disconnected")
        }
    }
}
