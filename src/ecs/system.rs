/// Module that holds all systems used by the ECS.
mod cleaner;
mod connection_manager;
mod event_receiver;
mod event_sender;
mod settings_manager;
mod user_manager;

pub use cleaner::*;
pub use connection_manager::*;
pub use event_receiver::*;
pub use event_sender::*;
pub use settings_manager::*;
pub use user_manager::*;

use shipyard::prelude::{Entities, ViewMut};
use tracing::{debug, trace};

use crate::ecs::component::OutgoingEvent;

pub fn send_event(
    event: OutgoingEvent,
    outgoing_events: &mut ViewMut<OutgoingEvent>,
    entities: &mut Entities,
) {
    debug!("Created outgoing event {}", event.0);
    trace!("Event data: {:?}", event.0);
    entities.add_entity(outgoing_events, event);
}
