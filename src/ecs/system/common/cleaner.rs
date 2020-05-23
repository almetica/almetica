use crate::ecs::message::EcsMessage;
use crate::ecs::resource::DeletionList;
use shipyard::*;
use tracing::trace;

/// The message cleaner cleans up all incoming messages amd other entities marked for deletion.
pub fn cleaner_system(mut all_storages: AllStoragesViewMut) {
    let mut deletion_list = all_storages
        .borrow::<UniqueViewMut<DeletionList>>()
        .0
        .clone();

    // Incoming message
    let mut list: Vec<EntityId> = all_storages
        .borrow::<View<EcsMessage>>()
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
    use crate::ecs::message::Message;
    use crate::protocol::packet::CPong;

    fn setup() -> World {
        let world = World::new();
        world.add_unique(DeletionList(vec![]));
        world
    }

    #[test]
    fn test_clean_incoming_message() {
        let world = setup();
        let connection_global_world_id = world.borrow::<EntitiesViewMut>().add_entity((), ());

        world.run(
            |(mut entities, mut messages): (EntitiesViewMut, ViewMut<EcsMessage>)| {
                for _i in 0..10 {
                    entities.add_entity(
                        &mut messages,
                        Box::new(Message::RequestPong {
                            connection_global_world_id,
                            packet: CPong {},
                        }),
                    );
                }
            },
        );

        let old_count = world.borrow::<View<EcsMessage>>().iter().count();
        assert_eq!(old_count, 10);

        world.run(cleaner_system);

        let new_count = world.borrow::<View<EcsMessage>>().iter().count();
        assert_eq!(new_count, 0);
    }
}
