use shipyard::*;
use tracing::{debug, error, info_span};

use crate::ecs::component::{IncomingEvent, Settings};
use crate::ecs::event::Event;
use crate::ecs::resource::WorldId;
use crate::protocol::packet::CSetVisibleRange;

/// The settings manager handles the settings of an account (UI/Chat/Visibility etc.).
pub fn settings_manager_system(
    events: View<IncomingEvent>,
    mut settings: ViewMut<Settings>,
    mut entities: EntitiesViewMut,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    (&events).iter().for_each(|event| {
        match &*event.0 {
            Event::RequestSetVisibleRange {
                connection_id,
                packet,
            } => {
                handle_set_visible_range(*connection_id, &packet, &mut settings, &mut entities);
            }
            _ => { /* Ignore all other events */ }
        }
    });
}

fn handle_set_visible_range(
    connection_id: Option<EntityId>,
    packet: &CSetVisibleRange,
    mut settings: &mut ViewMut<Settings>,
    entities: &mut EntitiesViewMut,
) {
    if let Some(connection_id) = connection_id {
        let span = info_span!("connection", connection = ?connection_id);
        let _enter = span.enter();

        debug!("Set visible range event incoming");

        // TODO The local world need to know of this values. Send this value once the user enters the local world.
        if let Ok(mut settings) = (&mut settings).try_get(connection_id) {
            settings.visibility_range = packet.range;
        } else {
            let user_settings = Settings {
                visibility_range: packet.range,
            };
            entities.add_entity(settings, user_settings);
        }
    } else {
        error!("Entity of the connection for set visible range event was not set");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use shipyard::*;

    use crate::ecs::component::Connection;
    use crate::ecs::event::Event;

    use super::*;

    fn setup() -> (World, EntityId) {
        let world = World::new();
        world.add_unique(WorldId(0));

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        (world, connection_id)
    }

    #[test]
    fn test_set_visible_range() {
        let (world, connection_id) = setup();

        world.run(
            |mut entities: EntitiesViewMut, mut events: ViewMut<IncomingEvent>| {
                entities.add_entity(
                    &mut events,
                    IncomingEvent(Arc::new(Event::RequestSetVisibleRange {
                        connection_id: Some(connection_id),
                        packet: CSetVisibleRange { range: 4234 },
                    })),
                );
            },
        );

        world.run(settings_manager_system);

        let valid_component_count = world
            .borrow::<View<Settings>>()
            .iter()
            .filter(|component| component.visibility_range > 0)
            .count();

        assert_eq!(valid_component_count, 1);
    }
}
