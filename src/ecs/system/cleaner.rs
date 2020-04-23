use shipyard::*;
use tracing::{info_span, trace};

use crate::ecs::component::{IncomingEvent, OutgoingEvent};
use crate::ecs::resource::{DeletionList, WorldId};

/// The event cleaner cleans up all events in the current ECS.
pub fn cleaner_system(mut all_storages: AllStoragesViewMut) {
    let world_id = all_storages.borrow::<UniqueView<WorldId>>().0;
    let span = info_span!("world", world_id = world_id);
    let _enter = span.enter();

    let mut deletion_list = all_storages
        .borrow::<UniqueViewMut<DeletionList>>()
        .0
        .clone();

    // Incoming event
    let mut list: Vec<EntityId> = all_storages
        .borrow::<View<IncomingEvent>>()
        .iter()
        .with_id()
        .map(|(id, _)| id)
        .collect();
    deletion_list.append(&mut list);

    // Outgoing event
    let mut list: Vec<EntityId> = all_storages
        .borrow::<View<OutgoingEvent>>()
        .iter()
        .with_id()
        .map(|(id, _)| id)
        .collect();
    deletion_list.append(&mut list);

    trace!("Deleting {} entities", deletion_list.len());

    // Delete entities that the other system marked for deletion.
    for id in deletion_list {
        all_storages.delete(id);
    }

    // Flush deletion list
    all_storages
        .borrow::<UniqueViewMut<DeletionList>>()
        .0
        .clear();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shipyard::*;

    use crate::ecs::component::IncomingEvent;
    use crate::ecs::event::Event;
    use crate::protocol::packet::CPong;

    use super::*;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));
        world
    }

    #[test]
    fn test_clean_incoming_event() {
        let world = setup();

        world.run(
            |(mut entities, mut events): (EntitiesViewMut, ViewMut<IncomingEvent>)| {
                for _i in 0..10 {
                    entities.add_entity(
                        &mut events,
                        IncomingEvent(Arc::new(Event::RequestPong {
                            connection_id: None,
                            packet: CPong {},
                        })),
                    );
                }
            },
        );

        let old_count = world.borrow::<View<IncomingEvent>>().iter().count();
        assert_eq!(old_count, 10);

        world.run(cleaner_system);

        let new_count = world.borrow::<View<IncomingEvent>>().iter().count();
        assert_eq!(new_count, 0);
    }

    #[test]
    fn test_clean_outgoing_event() {
        let world = setup();

        world.run(
            |(mut entities, mut events): (EntitiesViewMut, ViewMut<OutgoingEvent>)| {
                for _i in 0..10 {
                    entities.add_entity(
                        &mut events,
                        OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
                            connection_id: None,
                        })),
                    );
                }
            },
        );

        let old_count = world.borrow::<View<OutgoingEvent>>().iter().count();
        assert_eq!(old_count, 10);

        world.run(cleaner_system);

        let new_count = world.borrow::<View<OutgoingEvent>>().iter().count();
        assert_eq!(new_count, 0);
    }
}
