use std::sync::Arc;

use crate::ecs::event::Event;
use legion::prelude::*;
use legion::systems::SystemBuilder;
use legion::world::WorldId;
use tracing::trace;

/// The event cleaner cleans up all Events in the current ECS.
pub fn init(world_id: WorldId) -> Box<dyn Schedulable> {
    SystemBuilder::new("EventCleaner")
        .with_query(<Write<Arc<Event>>>::query())
        .build(move |command_buffer, world, _resources, queries| {
            for (entity, event) in queries.iter_entities_mut(&mut *world) {
                trace!("Deleted event {} on {:?}", *event, world_id);
                command_buffer.delete(entity);
            }
        })
}

// TODO cleaner test
