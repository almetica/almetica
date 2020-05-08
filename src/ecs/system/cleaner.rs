use crate::ecs::event::EcsEvent;
use crate::ecs::resource::{DeletionList, WorldId};
use shipyard::*;
use tracing::{info_span, trace};

/// The event cleaner cleans up all incoming events amd other entities marked for deletion.
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
        .borrow::<View<EcsEvent>>()
        .iter()
        .with_id()
        .map(|(id, _)| id)
        .collect();
    deletion_list.append(&mut list);

    if !deletion_list.is_empty() {
        trace!("Deleting {} entities", deletion_list.len());
    }

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
    use super::*;
    use crate::ecs::event::Event;
    use crate::protocol::packet::CPong;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(WorldId(0));
        world.add_unique(DeletionList(vec![]));
        world
    }

    #[test]
    fn test_clean_incoming_event() {
        let world = setup();
        let connection_id = world.borrow::<EntitiesViewMut>().add_entity((), ());

        world.run(
            |(mut entities, mut events): (EntitiesViewMut, ViewMut<EcsEvent>)| {
                for _i in 0..10 {
                    entities.add_entity(
                        &mut events,
                        Box::new(Event::RequestPong {
                            connection_id,
                            packet: CPong {},
                        }),
                    );
                }
            },
        );

        let old_count = world.borrow::<View<EcsEvent>>().iter().count();
        assert_eq!(old_count, 10);

        world.run(cleaner_system);

        let new_count = world.borrow::<View<EcsEvent>>().iter().count();
        assert_eq!(new_count, 0);
    }
}
