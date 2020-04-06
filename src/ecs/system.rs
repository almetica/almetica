pub mod connection_manager;
pub mod event_cleaner;
pub mod event_receiver;
pub mod event_sender;
pub mod settings_manager;
pub mod user_manager;

use crate::ecs::component::SingleEvent;
use crate::ecs::event::EventKind;
use crate::ecs::tag;
use legion::prelude::*;
use tracing::{debug, trace};

fn send_event(event: SingleEvent, command_buffer: &mut CommandBuffer) {
    debug!("Created {} event", event);
    trace!("Event data: {}", event);
    command_buffer
        .start_entity()
        .with_tag((tag::EventKind(EventKind::Response),))
        .with_component((event,))
        .build();
}
