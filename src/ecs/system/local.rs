/// All systems used by the local world
pub mod user_gateway;

pub use user_gateway::user_gateway_system;

use crate::ecs::component::LocalConnection;
use crate::ecs::message::EcsMessage;
use crate::ecs::system::send_message;
use tracing::{debug, error};

/// Send an outgoing packet message. This function can't be used by "Special Messages".
pub fn send_message_to_connection<'a, T>(message: EcsMessage, connections: T)
where
    T: shipyard::Get<Out = &'a LocalConnection>,
{
    if let Some(connection_id) = message.connection_id() {
        if let Ok(connection) = connections.try_get(connection_id) {
            send_message(message, &connection.channel);
        } else {
            debug!("Couldn't find user spawn: {:?}", connection_id);
        }
    } else {
        error!("Message didn't had a local world ID attached");
    }
}
