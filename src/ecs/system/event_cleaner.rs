/// The event cleaner cleans up all Events in the current ECS.
use crate::ecs::component::{SingleEvent, BatchEvent};

use legion::prelude::*;
use legion::systems::SystemBuilder;
use tracing::{info_span, trace};

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventCleaner")
        .with_query(<Write<SingleEvent>>::query())
        .with_query(<Write<BatchEvent>>::query())
        .build(move |command_buffer, world, _resources, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            // Single event
            for (entity, event) in queries.0.iter_entities_mut(&mut *world) {
                trace!("Deleted event {}", *event);
                command_buffer.delete(entity);
            }

            // Batch event
            for (entity, event) in queries.1.iter_entities_mut(&mut *world) {
                trace!("Deleted batch event with {} events", event.len());
                command_buffer.delete(entity);
            }
        })
}

// TODO cleaner test
