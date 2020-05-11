/// Events that are handled / emitted by the ECS.
///
/// Events are only to be used for communication between
/// ECS and connections (and ECS to ECS). They are not to be used for ECS internal communication.
/// For this, you should use other components.
///
/// There are packet and system events.
/// System Events are special events that don't send out a packet to the client and are normally
/// handling the state between the server systems.
///
/// A event always has a target: Global ECS, local ECS or a connection.
///
/// Messages from the connections  to the ECS are always requests.
/// Messages from the ECS to the Connections are always responses.
/// Messages between the ECS can be either request or response.
///
/// Network connections and ECS have a channel to write events into.
///
use crate::protocol::opcode::Opcode;
use crate::protocol::packet::*;
use crate::protocol::serde::{from_vec, to_vec};
use crate::{AlmeticaError, Result};
use anyhow::bail;
use async_std::sync::Sender;
use shipyard::*;
use std::fmt;

/// ECS events. We use `Box` so that we don't need to copy packet data around.
pub type EcsEvent = Box<Event>;

/// The target of the event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventTarget {
    Global,
    Local,
    Connection,
}

macro_rules! assemble_event {
    (
    Authenticated Packet Events {
        $($a_ty:ident{packet: $a_packet_type:ty $(, $a_arg_name:ident: $a_arg_type:ty)*}, $a_opcode:ident, $a_target:ident;)*
    }
    Unauthenticated Packet Events {
        $($u_ty:ident{packet: $u_packet_type:ty $(, $u_arg_name:ident: $u_arg_type:ty)*}, $u_opcode:ident, $u_target:ident;)*
    }
    System Events {
        $($s_ty:ident{$($s_arg_name:ident: $s_arg_type:ty)*}, $s_target:ident;)*
    }
    ) => {
        /// Event enum for all events.
        #[derive(Clone, Debug)]
        pub enum Event {
            RequestRegisterConnection{response_channel: Sender<Box<Event>>},
            $($a_ty {connection_id: EntityId, account_id: i64, packet: $a_packet_type $(,$a_arg_name: $a_arg_type)*},)*
            $($u_ty {connection_id: EntityId, packet: $u_packet_type $(,$u_arg_name: $u_arg_type)*},)*
            $($s_ty {connection_id: EntityId, $($s_arg_name: $s_arg_type),*},)*
        }

        impl Event {
            /// Creates a new Request/Response event for the given opcode & packet data.
            pub fn new_from_packet(connection_id: EntityId, account_id: Option<i64>, opcode: Opcode, packet_data: Vec<u8>) -> Result<Event> {
                match opcode {
                    $(Opcode::$a_opcode => {
                        if account_id.is_none() {
                            bail!(AlmeticaError::UnauthorizedPacket);
                        }

                        let packet = from_vec(packet_data)?;
                        Ok(Event::$a_ty{connection_id, account_id: account_id.unwrap(), packet})
                    },)*
                    $(Opcode::$u_opcode => {
                        let packet = from_vec(packet_data)?;
                        Ok(Event::$u_ty{connection_id: connection_id, packet})
                    },)*
                    _ => bail!(AlmeticaError::NoEventMappingForPacket),
                }
            }

            /// Get the connection id of a packet event.
            pub fn connection_id(&self) -> Option<EntityId> {
                match self {
                    Event::RequestRegisterConnection{..} => None,
                    $(Event::$a_ty{connection_id,..} => Some(*connection_id),)*
                    $(Event::$u_ty{connection_id,..} => Some(*connection_id),)*
                    $(Event::$s_ty{connection_id,..} => Some(*connection_id),)*
                }
            }

            /// Get the data from a packet event.
            pub fn data(&self) -> Result<Option<Vec<u8>>> {
                match self {
                    $(Event::$a_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    $(Event::$u_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    _ => Ok(None),
                }
            }

            /// Get the opcode from a packet event.
            pub fn opcode(&self) -> Option<Opcode> {
                match self {
                    $(Event::$a_ty{..} => {
                        Some(Opcode::$a_opcode)
                    },)*
                    $(Event::$u_ty{..} => {
                        Some(Opcode::$u_opcode)
                    },)*
                    _ => None,
                }
            }

            /// Get the target of the event (global world / local world / connection).
            pub fn target(&self) -> EventTarget {
                match self {
                    Event::RequestRegisterConnection{..} => EventTarget::Global,
                    $(Event::$a_ty{..} => EventTarget::$a_target,)*
                    $(Event::$u_ty{..} => EventTarget::$u_target,)*
                    $(Event::$s_ty{..} => EventTarget::$s_target,)*
                }
            }
        }

        impl fmt::Display for Event {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    Event::RequestRegisterConnection{..} => write!(f, "{}", stringify!(RequestRegisterConnection)),
                    $(Event::$a_ty{..} => write!(f, "{}", stringify!($a_ty)),)*
                    $(Event::$u_ty{..} => write!(f, "{}", stringify!($u_ty)),)*
                    $(Event::$s_ty{..} => write!(f, "{}", stringify!($s_ty)),)*
                }
            }
        }
    };
}

assemble_event! {
    // Packets that need an account ID attached.
    Authenticated Packet Events {
        RequestCanCreateUser{packet: CCanCreateUser}, C_CAN_CREATE_USER, Global;
        RequestCheckUserName{packet: CCheckUserName}, C_CHECK_USERNAME, Global;
        RequestCreateUser{packet: CCreateUser}, C_CREATE_USER, Global;
        RequestGetUserList{packet: CGetUserList}, C_GET_USER_LIST, Global;
        RequestSetVisibleRange{packet: CSetVisibleRange}, C_SET_VISIBLE_RANGE, Global;
        ResponseLoginArbiter{packet: SLoginArbiter}, S_LOGIN_ARBITER, Connection;
    }
    // Packet events that don't need an account ID attached.
    Unauthenticated Packet Events {
        RequestLoginArbiter{packet: CLoginArbiter}, C_LOGIN_ARBITER, Global;
        RequestCheckVersion{packet: CCheckVersion}, C_CHECK_VERSION, Global;
        RequestPong{packet: CPong}, C_PONG, Global;
        ResponseCanCreateUser{packet: SCanCreateUser}, S_CAN_CREATE_USER, Connection;
        ResponseCheckUserName{packet: SCheckUserName}, S_CHECK_USERNAME, Connection;
        ResponseCheckVersion{packet: SCheckVersion}, S_CHECK_VERSION, Connection;
        ResponseCreateUser{packet: SCreateUser}, S_CREATE_USER, Connection;
        ResponseGetUserList{packet: SGetUserList}, S_GET_USER_LIST, Global;
        ResponseLoadingScreenControlInfo{packet: SLoadingScreenControlInfo}, S_LOADING_SCREEN_CONTROL_INFO, Connection;
        ResponseLoginAccountInfo{packet: SLoginAccountInfo}, S_LOGIN_ACCOUNT_INFO, Connection;
        ResponsePing{packet: SPing}, S_PING, Connection;
        ResponseRemainPlayTime{packet: SRemainPlayTime}, S_REMAIN_PLAY_TIME, Connection;
    }
    // System events are all packets that are not de-/serialized from/to a packet.
    System Events {
        // The connection will get it's EntityId returned with this message after registration.
        ResponseRegisterConnection{}, Connection;
        // The connection will be dropped after it receives this message.
        ResponseDropConnection{}, Connection;
    }
}

#[cfg(test)]
mod tests {
    use async_std::sync::channel;
    use shipyard::*;

    use crate::model::Region;
    use crate::protocol::opcode::Opcode;

    use super::*;

    #[test]
    fn test_opcode_mapping() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());

        let data = vec![
            0x2, 0x0, 0x8, 0x0, 0x8, 0x0, 0x14, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1d, 0x8a, 0x5, 0x0,
            0x14, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0xce, 0x7b, 0x5, 0x0,
        ];
        let event = Event::new_from_packet(entity, None, Opcode::C_CHECK_VERSION, data)?;
        if let Event::RequestCheckVersion {
            connection_id: entity_id,
            packet,
        } = event
        {
            assert_eq!(entity, entity_id);
            assert_eq!(packet.version[0].index, 0);
            assert_eq!(packet.version[0].value, 363_037);
            assert_eq!(packet.version[1].index, 1);
            assert_eq!(packet.version[1].value, 359_374);
        } else {
            panic!("New didn't returned the right event.");
        }
        Ok(())
    }

    #[test]
    fn test_unauthorized_packet_creation() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());

        let data = vec![
            0x6, 0x0, 0x54, 0x0, 0x68, 0x0, 0x65, 0x0, 0x42, 0x0, 0x65, 0x0, 0x73, 0x0, 0x74, 0x0,
            0x4e, 0x0, 0x61, 0x0, 0x6d, 0x0, 0x65, 0x0, 0x0, 0x0,
        ];

        match Event::new_from_packet(entity, None, Opcode::C_CHECK_USERNAME, data) {
            Ok(..) => panic!("Could create an authenticated packet without an account ID"),
            Err(e) => match e.downcast_ref::<AlmeticaError>() {
                Some(AlmeticaError::UnauthorizedPacket) => Ok(()),
                Some(..) => panic!(e),
                None => panic!(e),
            },
        }
    }

    #[test]
    fn test_target_global() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Event::RequestLoginArbiter {
            connection_id: entity,
            packet: CLoginArbiter {
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                region: Region::Europe,
                patch_version: 0,
            },
        };
        assert_eq!(org.target(), EventTarget::Global);
        Ok(())
    }

    #[test]
    fn test_target_connection() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Event::ResponseCheckVersion {
            connection_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.target(), EventTarget::Connection);
        Ok(())
    }

    #[test]
    fn test_event_opcode_some() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Event::ResponseCheckVersion {
            connection_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.opcode(), Some(Opcode::S_CHECK_VERSION));
        Ok(())
    }

    #[test]
    fn test_event_opcode_none() -> Result<()> {
        let (response_channel, _) = channel(1);
        let org = Event::RequestRegisterConnection { response_channel };

        assert_eq!(org.opcode(), None);
        Ok(())
    }

    #[test]
    fn test_event_connection_some() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Event::ResponseCheckVersion {
            connection_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.connection_id(), Some(entity));
        Ok(())
    }

    #[test]
    fn test_event_connection_none() -> Result<()> {
        let (response_channel, _) = channel(1);
        let org = Event::RequestRegisterConnection { response_channel };

        assert_eq!(org.connection_id(), None);
        Ok(())
    }
}
