/// The event cleaner cleans up all Events in the current ECS.
use shipyard::prelude::*;
use tracing::{info_span, trace};

use crate::ecs::component::{IncomingEvent, OutgoingEvent};
use crate::ecs::resource::{DeletionList, WorldId};

pub struct Cleaner;

impl<'sys> System<'sys> for Cleaner {
    type Data = AllStorages;

    fn run(mut all_storages: <Self::Data as SystemData<'sys>>::View) {
        let world_id = all_storages.borrow::<Unique<&WorldId>>().0;
        let span = info_span!("world", world_id = world_id);
        let _enter = span.enter();

        let mut deletion_list = all_storages.borrow::<Unique<&mut DeletionList>>().0.clone();

        // Incoming event
        let mut list: Vec<EntityId> = all_storages
            .borrow::<&IncomingEvent>()
            .iter()
            .with_id()
            .map(|(id, _)| id)
            .collect();
        deletion_list.append(&mut list);

        // Outgoing event
        let mut list: Vec<EntityId> = all_storages
            .borrow::<&OutgoingEvent>()
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
        all_storages.borrow::<Unique<&mut DeletionList>>().0.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shipyard::prelude::*;

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

        world.run::<(EntitiesMut, &mut IncomingEvent), _, _>(|(mut entities, mut events)| {
            for _i in 0..10 {
                entities.add_entity(
                    &mut events,
                    IncomingEvent(Arc::new(Event::RequestPong {
                        connection_id: None,
                        packet: CPong {},
                    })),
                );
            }
        });

        let old_count = world.borrow::<&IncomingEvent>().iter().count();
        assert_eq!(10, old_count);

        world.run_system::<Cleaner>();

        let new_count = world.borrow::<&IncomingEvent>().iter().count();
        assert_eq!(0, new_count);
    }

    #[test]
    fn test_clean_outgoing_event() {
        let world = setup();

        world.run::<(EntitiesMut, &mut OutgoingEvent), _, _>(|(mut entities, mut events)| {
            for _i in 0..10 {
                entities.add_entity(
                    &mut events,
                    OutgoingEvent(Arc::new(Event::ResponseRegisterConnection {
                        connection_id: None,
                    })),
                );
            }
        });

        let old_count = world.borrow::<&OutgoingEvent>().iter().count();
        assert_eq!(10, old_count);

        world.run_system::<Cleaner>();

        let new_count = world.borrow::<&OutgoingEvent>().iter().count();
        assert_eq!(0, new_count);
    }
}
