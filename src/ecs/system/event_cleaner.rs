use std::sync::Arc;

use crate::ecs::event::Event;
use legion::prelude::*;
use legion::systems::SystemBuilder;
use tracing::{info_span, trace};

/// The event cleaner cleans up all Events in the current ECS.
pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventCleaner")
        .with_query(<Write<Arc<Event>>>::query())
        .build(move |command_buffer, world, _resources, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            for (entity, event) in queries.iter_entities_mut(&mut *world) {
                trace!("Deleted event {}", *event);
                command_buffer.delete(entity);
            }
        })
}

// TODO cleaner test
