use super::resource::EventRxChannel;

use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use legion::world::World;
use log::{debug, error};
use tokio::sync::mpsc::error::TryRecvError;

/// Event dispatcher dispatches Events from the Request channel into the ECS.
pub fn event_dispatcher(world: &World) -> Box<dyn Schedulable> {
    let world_id = world.id();
    SystemBuilder::new("ConnectionSystem")
        .write_resource::<EventRxChannel>()
        .build(move |command_buffer, _sub_world, event_channel, _query| {
            loop {
                match event_channel.rx_channel.try_recv() {
                    Ok(event) => {
                        debug!("Received event {} for {:?}", event, world_id);
                        //command_buffer.insert((), (0..1).map(move |_| (event.clone(),)));
                        command_buffer.start_entity()
                            .with_component((*event,))
                            .build();
                    }
                    Err(e) => {
                        match e {
                            TryRecvError::Empty => { /* Nothing to do */ }
                            TryRecvError::Closed => {
                                error!("Request channel was closed for {:?}", world_id);
                            }
                        }
                    }
                }
            }
        })
}

// TODO The connection system needs to take the connection registration and create
// a ID for the connection. This ID needs to be send back to the connection and needs
// to be used after this point for every event. The system saves the id -> channel
// mapping into a HashMap resource. This connection ID could later serves as the
// user ID for all intense and purposes.

