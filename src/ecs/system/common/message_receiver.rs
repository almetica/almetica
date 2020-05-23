use crate::ecs::message::{EcsMessage, Message};
use crate::ecs::resource::{InputChannel, ShutdownSignal, ShutdownSignalStatus};
use async_std::sync::TryRecvError;
use shipyard::*;
use tracing::{debug, info, trace};

// TODO test the setting of ShutdownSignalStatus::ShutdownInProgress

/// Message receiver dispatches the messages from the request channel into the ECS.
pub fn message_receiver_system(
    mut incoming_messages: ViewMut<EcsMessage>,
    mut entities: EntitiesViewMut,
    message_channel: UniqueView<InputChannel>,
    mut shutdown: UniqueViewMut<ShutdownSignal>,
) {
    loop {
        match message_channel.channel.try_recv() {
            Ok(message) => match *message {
                Message::ShutdownSignal { .. } => {
                    info!("Setting shutdown signal to status ShutdownSignalStatus::ShutdownInProgress");
                    shutdown.status = ShutdownSignalStatus::ShutdownInProgress;
                }
                _ => {
                    debug!("Created incoming {}", message);
                    trace!("Message data: {:?}", message);
                    entities.add_entity(&mut incoming_messages, message);
                }
            },
            Err(TryRecvError::Empty) => {
                break;
            }
            Err(TryRecvError::Disconnected) => panic!("Message channel was disconnected"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::message::Message;
    use crate::ecs::resource::InputChannel;
    use crate::protocol::packet::CCheckVersion;
    use crate::Result;
    use async_std::sync::channel;

    #[test]
    fn test_message_receiving() -> Result<()> {
        let world = World::new();

        let (tx_channel, rx_channel) = channel(10);

        world.add_unique(InputChannel {
            channel: rx_channel,
        });

        world.add_unique(ShutdownSignal {
            status: ShutdownSignalStatus::Operational,
        });

        let entity = world.borrow::<EntitiesViewMut>().add_entity((), ());

        tx_channel.try_send(Box::new(Message::RequestCheckVersion {
            connection_global_world_id: entity,
            packet: CCheckVersion { version: vec![] },
        }))?;
        tx_channel.try_send(Box::new(Message::RequestCheckVersion {
            connection_global_world_id: entity,
            packet: CCheckVersion { version: vec![] },
        }))?;

        world.run(message_receiver_system);

        let count = world.borrow::<View<EcsMessage>>().iter().count();
        assert_eq!(count, 2);

        Ok(())
    }
}
