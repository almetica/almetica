use crate::ecs::component::Settings;
use crate::ecs::message::{EcsMessage, Message};
use crate::protocol::packet::CSetVisibleRange;
use shipyard::*;
use tracing::{debug, info_span};

/// The settings manager handles the settings of an account (UI/Chat/Visibility etc.).
pub fn settings_manager_system(
    messages: View<EcsMessage>,
    mut settings: ViewMut<Settings>,
    mut entities: EntitiesViewMut,
) {
    (&messages).iter().for_each(|message| {
        match &**message {
            Message::RequestSetVisibleRange {
                connection_global_world_id,
                packet,
                ..
            } => {
                id_span!(connection_global_world_id);
                handle_set_visible_range(
                    *connection_global_world_id,
                    &packet,
                    &mut settings,
                    &mut entities,
                );
            }
            _ => { /* Ignore all other messages */ }
        }
    });
}

fn handle_set_visible_range(
    connection_global_world_id: EntityId,
    packet: &CSetVisibleRange,
    mut settings: &mut ViewMut<Settings>,
    entities: &mut EntitiesViewMut,
) {
    debug!("Message::RequestSetVisibleRange incoming");

    // TODO The local world need to know of this values. Send this value once the user enters the local world.
    if let Ok(mut settings) = (&mut settings).try_get(connection_global_world_id) {
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
    use crate::ecs::component::GlobalConnection;
    use crate::ecs::message::Message;
    use async_std::sync::{channel, Receiver};
    use std::time::Instant;

    fn setup_with_connection() -> (World, EntityId, Receiver<EcsMessage>) {
        let world = World::new();

        let (tx_channel, rx_channel) = channel(1024);

        let connection_global_world_id = world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<GlobalConnection>| {
                entities.add_entity(
                    &mut connections,
                    GlobalConnection {
                        channel: tx_channel,
                        is_version_checked: false,
                        is_authenticated: false,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        );

        (world, connection_global_world_id, rx_channel)
    }

    #[test]
    fn test_set_visible_range() {
        let (world, connection_global_world_id, _rx_channel) = setup_with_connection();

        world.run(
            |mut entities: EntitiesViewMut, mut messages: ViewMut<EcsMessage>| {
                entities.add_entity(
                    &mut messages,
                    Box::new(Message::RequestSetVisibleRange {
                        connection_global_world_id,
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
