/// The event cleaner cleans up all Events in the current ECS.
use crate::ecs::component::{BatchEvent, SingleEvent};

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::ecs::component::SingleEvent;
    use crate::ecs::event::{self, Event};
    use crate::ecs::tag::EventKind;
    use crate::ecs::world::Multiverse;

    fn setup_world() -> (Multiverse, Schedule) {
        let multiverse = Multiverse::new();
        let schedule = Schedule::builder()
            .add_system(init(multiverse.global_world_handle.world.id().index()))
            .build();
        (multiverse, schedule)
    }

    #[test]
    fn test_event_cleaner_single_event() {
        let (mut multiverse, mut schedule) = setup_world();
        let mut world = &mut multiverse.global_world_handle.world;

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..10).map(|_| (Arc::new(Event::ResponseRegisterConnection { connection: None }),)),
        );

        let query = <(Read<SingleEvent>,)>::query();

        let old_count = query.iter(&mut world).count();
        assert_eq!(10, old_count);

        schedule.execute(&mut world, &mut multiverse.resources);

        let new_count = query.iter(&mut world).count();
        assert_eq!(0, new_count);
    }

    #[test]
    fn test_event_cleaner_batch_event() {
        let (mut multiverse, mut schedule) = setup_world();
        let mut world = &mut multiverse.global_world_handle.world;

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..10).map(|_| (vec![
                Arc::new(Event::ResponseRegisterConnection { connection: None }),
                Arc::new(Event::ResponseDropConnection { connection: None }),
                Arc::new(Event::ResponseRegisterConnection { connection: None }),
                Arc::new(Event::ResponseDropConnection { connection: None }),
            ],)),
        );

        let query = <(Read<BatchEvent>,)>::query();

        let old_count = query.iter(&mut world).count();
        assert_eq!(10, old_count);

        schedule.execute(&mut world, &mut multiverse.resources);

        let new_count = query.iter(&mut world).count();
        assert_eq!(0, new_count);
    }
}
