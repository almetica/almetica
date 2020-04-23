/// Module that holds all systems used by the ECS.
mod cleaner;
mod connection_manager;
mod event_receiver;
mod event_sender;
mod settings_manager;
mod user_manager;

pub use cleaner::cleaner_system;
pub use connection_manager::connection_manager_system;
pub use event_receiver::event_receiver_system;
pub use event_sender::event_sender_system;
pub use settings_manager::settings_manager_system;
pub use user_manager::user_manager_system;

use shipyard::*;
use tracing::{debug, trace};

use crate::ecs::component::OutgoingEvent;

/// Send an outgoing event.
pub fn send_event(
    event: OutgoingEvent,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut EntitiesViewMut,
) {
    debug!("Created outgoing event {}", event.0);
    trace!("Event data: {:?}", event.0);
    entities.add_entity(outgoing_events, event);
}
