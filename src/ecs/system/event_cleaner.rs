use std::sync::Arc;

use crate::ecs::event::Event;
use legion::prelude::*;
use legion::systems::SystemBuilder;

/// The event cleaner cleans up all Events in the current ECS.
pub fn init() -> Box<dyn Schedulable> {
    SystemBuilder::new("EventCleaner")
        .with_query(<Write<Arc<Event>>>::query())
        .build(move |command_buffer, world, _resources, queries| {
            for (entity, _) in queries.iter_entities_mut(&mut *world) {
                command_buffer.delete(entity);
            }
        })
}

// TODO cleaner test
