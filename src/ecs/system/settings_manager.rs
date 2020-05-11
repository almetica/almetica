use crate::ecs::component::Settings;
use crate::ecs::event::{EcsEvent, Event};
use crate::ecs::resource::WorldId;
use crate::protocol::packet::CSetVisibleRange;
use shipyard::*;
use tracing::{debug, info_span};

/// The settings manager handles the settings of an account (UI/Chat/Visibility etc.).
pub fn settings_manager_system(
    events: View<EcsEvent>,
    mut settings: ViewMut<Settings>,
    mut entities: EntitiesViewMut,
    world_id: UniqueView<WorldId>,
) {
    let span = info_span!("world", world_id = world_id.0);
    let _enter = span.enter();

    (&events).iter().for_each(|event| {
        match &**event {
            Event::RequestSetVisibleRange {
                connection_id,
                packet,
                ..
            } => {
                handle_set_visible_range(*connection_id, &packet, &mut settings, &mut entities);
            }
            _ => { /* Ignore all other events */ }
        }
    });
}

fn handle_set_visible_range(
    connection_id: EntityId,
    packet: &CSetVisibleRange,
    mut settings: &mut ViewMut<Settings>,
    entities: &mut EntitiesViewMut,
) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::component::Connection;
    use crate::ecs::event::Event;
    use async_std::sync::{channel, Receiver};
    use std::time::Instant;

    fn setup_with_connection() -> (World, EntityId, Receiver<EcsEvent>) {
        let world = World::new();
        world.add_unique(WorldId(0));

        let (tx_channel, rx_channel) = channel(1024);

        let connection_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        channel: tx_channel,
                        account_id: None,
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        (world, connection_id, rx_channel)
    }

    #[test]
    fn test_set_visible_range() {
        let (world, connection_id, _rx_channel) = setup_with_connection();

        world.run(
            |mut entities: EntitiesViewMut, mut events: ViewMut<EcsEvent>| {
                entities.add_entity(
                    &mut events,
                    Box::new(Event::RequestSetVisibleRange {
                        connection_id,
                        account_id: -1,
                        packet: CSetVisibleRange { range: 4234 },
                    }),
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
