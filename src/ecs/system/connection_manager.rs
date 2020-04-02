use std::sync::Arc;

use crate::ecs::event::Event;
use crate::ecs::event::EventKind;
use crate::ecs::resource::ConnectionMapping;
use crate::ecs::tag;
use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use rand::rngs::OsRng;
use rand_core::RngCore;
use tracing::{debug, error, info_span};

/// Connection handler handles the connection components on user entities.
pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("ConnectionManager")
        .write_resource::<ConnectionMapping>()
        .with_query(<Read<Arc<Event>>>::query().filter(tag_value(&tag::EventKind(EventKind::Request))))
        .write_component::<Arc<Event>>()
        .build(move |command_buffer, world, connection_mapping, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            for event in queries.iter_mut(&mut *world) {
                match &**event {
                    Event::RequestRegisterConnection {
                        uid: 0,
                        response_channel,
                    } => {
                        debug!("Registration event incoming for");
                        let uid = OsRng.next_u64();
                        connection_mapping.map.insert(uid, response_channel.clone());
                        debug!("Registered connection with uid {}", uid);

                        let new_event = Arc::new(Event::ResponseRegisterConnection { uid });
                        debug!("Created {:?} event", new_event);
                        command_buffer
                            .start_entity()
                            .with_tag((tag::EventKind(EventKind::Response),))
                            .with_component((new_event,))
                            .build();
                    }
                    Event::RequestLoginArbiter { .. } => {
                        error!("NOT IMPLEMENTED YET1");
                    }
                    Event::RequestCheckVersion { .. } => {
                        error!("NOT IMPLEMENTED YET2");
                    }
                    _ => { /* Skip all other events */ }
                }
            }
        })
}

// TODO Registration test
