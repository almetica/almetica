/// The settings manager handles the settings of an account (UI/Chat/Visibility etc.).
use crate::ecs::component::{Settings, SingleEvent};
use crate::ecs::event::{Event, EventKind};
use crate::ecs::tag;
use crate::protocol::packet::CSetVisibleRange;

use legion::prelude::{tag_value, CommandBuffer, Entity, IntoQuery, Read};
use legion::systems::schedule::Schedulable;
use legion::systems::{SubWorld, SystemBuilder};
use tracing::{debug, error, info_span};

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("ConnectionManager")
        .with_query(<Read<SingleEvent>>::query().filter(tag_value(&tag::EventKind(EventKind::Request))))
        .write_component::<Settings>()
        .build(move |mut command_buffer, mut world, _resources, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            for event in queries.iter(&*world) {
                match &**event {
                    Event::RequestSetVisibleRange { connection, packet } => {
                        handle_set_visible_range(*connection, &packet, &mut world, &mut command_buffer);
                    }
                    _ => { /* Ignore all other events */ }
                }
            }
        })
}

fn handle_set_visible_range(
    connection: Option<Entity>,
    packet: &CSetVisibleRange,
    world: &mut SubWorld,
    command_buffer: &mut CommandBuffer,
) {
    if let Some(connection) = connection {
        let span = info_span!("connection", %connection);
        let _enter = span.enter();

        debug!("Set visible range event incoming");

        // TODO most likely the local world need to know of this values. Send this value once
        // the user enters the local world.
        if let Some(mut component) = world.get_component_mut::<Settings>(connection) {
            component.visibility_range = packet.range;
        } else {
            let settings = Settings {
                visibility_range: packet.range,
            };
            command_buffer.start_entity().with_component((settings,)).build();
        }
    } else {
        error!("Entity of the connection for set visible range event was not set");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::time::Instant;

    use crate::ecs::component::Connection;
    use crate::ecs::event::{self, Event};
    use crate::ecs::tag::EventKind;

    use legion::prelude::{Resources, World};
    use legion::query::Read;
    use legion::systems::schedule::Schedule;

    fn setup() -> (World, Schedule, Entity, Resources) {
        let mut world = World::new();
        let schedule = Schedule::builder().add_system(init(world.id().index())).build();

        // FIXME There currently isn't a good insert method for one entity.
        let entities = world.insert(
            (),
            (0..1).map(|_| {
                (Connection {
                    verified: false,
                    version_checked: false,
                    region: None,
                    last_pong: Instant::now(),
                },)
            }),
        );
        let connection = entities[0];
        let resources = Resources::default();

        (world, schedule, connection, resources)
    }

    #[test]
    fn test_set_visible_range() {
        let (mut world, mut schedule, connection, mut resources) = setup();

        world.insert(
            (EventKind(event::EventKind::Request),),
            (0..1).map(|_| {
                (Arc::new(Event::RequestSetVisibleRange {
                    connection: Some(connection),
                    packet: CSetVisibleRange { range: 4234 },
                }),)
            }),
        );

        schedule.execute(&mut world, &mut resources);

        let query = <Read<Settings>>::query();
        let valid_component_count = query
            .iter(&world)
            .filter(|component| component.visibility_range > 0)
            .count();

        assert_eq!(1, valid_component_count);
    }
}
