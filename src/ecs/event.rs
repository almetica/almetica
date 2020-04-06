/// Events that are handled / emitted by the ECS.
///
/// There are packet and system events.
///
/// A request can either be for the local or for the global ECS.
///
/// We also differentiate if a Event is an request or a response (from the ECS
/// perspective).
use std::fmt;
use std::sync::Arc;

use super::super::protocol::opcode::Opcode;
use super::super::protocol::packet::*;
use super::super::protocol::serde::{from_vec, to_vec};
use super::super::{Error, Result};

use legion::prelude::Entity;
use tokio::sync::mpsc::Sender;

/// The kind of an the event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    Request,
    Response,
}

/// The target of the event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventTarget {
    Global,
    Local,
    Connection,
}

macro_rules! assemble_event {
    (
    Packet Events {
        $($p_ty:ident{packet: $p_packet_type:ty $(, $p_arg_name:ident: $p_arg_type:ty)*}, $p_opcode:ident, $p_kind:ident, $p_target:ident;)*
    }
    System Events {
        $($e_ty:ident{$($e_arg_name:ident: $e_arg_type:ty)*}, $e_kind:ident, $e_target:ident;)*
    }
    ) => {
        /// Event enum for all events.
        #[derive(Clone, Debug)]
        pub enum Event {
            $($p_ty {connection: Option<Entity>, packet: $p_packet_type $(,$p_arg_name: $p_arg_type)*},)*
            $($e_ty {connection: Option<Entity>, $($e_arg_name: $e_arg_type),*},)*
        }

        impl Event {
            /// Creates a new Request/Response event for the given opcode & packet data.
            pub fn new_from_packet(connection: Entity, opcode: Opcode, packet_data: Vec<u8>) -> Result<Event> {
                match opcode {
                    $(Opcode::$p_opcode => {
                        let packet = from_vec(packet_data)?;
                        Ok(Event::$p_ty{connection: Some(connection), packet})
                    },)*
                    _ => Err(Error::NoEventMappingForPacket),
                }
            }

            /// Get the connection of a packet event.
            pub fn connection(&self) -> Option<Entity> {
                match self {
                    $(Event::$p_ty{connection,..} => {
                        *connection
                    },)*
                    $(Event::$e_ty{connection,..} => {
                        *connection
                    },)*
                }
            }

            /// Get the data from a packet event.
            pub fn data(&self) -> Result<Option<Vec<u8>>> {
                match self {
                    $(Event::$p_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    _ => Ok(None),
                }
            }

            /// Get the opcode from a packet event.
            pub fn opcode(&self) -> Option<Opcode> {
                match self {
                    $(Event::$p_ty{..} => {
                        Some(Opcode::$p_opcode)
                    },)*
                    _ => None,
                }
            }

            /// Get the target of the event (global world / local world / connection).
            pub fn target(&self) -> EventTarget {
                match self {
                    $(Event::$p_ty{..} => EventTarget::$p_target,)*
                    $(Event::$e_ty{..} => EventTarget::$e_target,)*
                }
            }

            /// Get the kind of the event (Request or Response).
            pub fn kind(&self) -> EventKind {
                match self {
                    $(Event::$p_ty{..} => EventKind::$p_kind,)*
                    $(Event::$e_ty{..} => EventKind::$e_kind,)*
                }
            }
        }

        impl fmt::Display for Event {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(Event::$p_ty{..} => write!(f, "{}", stringify!($p_ty)),)*
                    $(Event::$e_ty{..} => write!(f, "{}", stringify!($e_ty)),)*
                }
            }
        }
    };
}

assemble_event! {
    Packet Events {
        RequestLoginArbiter{packet: CLoginArbiter}, C_LOGIN_ARBITER, Request, Global;
        ResponseLoginArbiter{packet: SLoginArbiter}, S_LOGIN_ARBITER, Response, Connection;
        RequestCheckVersion{packet: CCheckVersion}, C_CHECK_VERSION, Request, Global;
        ResponseCheckVersion{packet: SCheckVersion}, S_CHECK_VERSION, Response, Connection;
        ResponseLoadingScreenControlInfo{packet: SLoadingScreenControlInfo}, S_LOADING_SCREEN_CONTROL_INFO, Response, Connection;
        ResponseRemainPlayTime{packet: SRemainPlayTime}, S_REMAIN_PLAY_TIME, Response, Connection;
        ResponseLoginAccountInfo{packet: SLoginAccountInfo}, S_LOGIN_ACCOUNT_INFO, Response, Connection;
        RequestSetVisibleRange{packet: CSetVisibleRange}, C_SET_VISIBLE_RANGE, Request, Global;
        RequestGetUserList{packet: CGetUserList}, C_GET_USER_LIST, Request, Global;
        ResponseGetUserList{packet: SGetUserList}, S_GET_USER_LIST, Request, Global;
    }
    System Events {
        // Registers the response channel of a connection at a world.
        RequestRegisterConnection{response_channel: Sender<Arc<Event>>}, Request, Global;
        // The connection will get it's uid returned with this message after registration.
        ResponseRegisterConnection{}, Response, Connection;
        // The connection will be dropped after it receive this message.
        ResponseDropConnection{}, Response, Connection;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Region;
    use crate::protocol::opcode::Opcode;
    use crate::Error;
    use legion::prelude::World;
    use tokio::sync::mpsc::channel;

    #[test]
    fn test_opcode_mapping() -> Result<(), Error> {
        let mut world = World::new();
        let entities = world.insert((), vec![(1,)]);
        let entity_id = entities[0];

        let data = vec![
            0x2, 0x0, 0x8, 0x0, 0x8, 0x0, 0x14, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1d, 0x8a, 0x5, 0x0, 0x14, 0x0, 0x0, 0x0,
            0x1, 0x0, 0x0, 0x0, 0xce, 0x7b, 0x5, 0x0,
        ];
        let event = Event::new_from_packet(entity_id, Opcode::C_CHECK_VERSION, data)?;
        if let Event::RequestCheckVersion {
            connection: _entity_id,
            packet,
        } = event
        {
            assert_eq!(0, packet.version[0].index);
            assert_eq!(363_037, packet.version[0].value);
            assert_eq!(1, packet.version[1].index);
            assert_eq!(359_374, packet.version[1].value);
        } else {
            panic!("New didn't returned the right event.");
        }
        Ok(())
    }

    #[test]
    fn test_target_global() -> Result<(), Error> {
        let org = Event::RequestLoginArbiter {
            connection: None,
            packet: CLoginArbiter {
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                region: Region::Europe,
                patch_version: 0,
            },
        };
        assert_eq!(EventTarget::Global, org.target());
        Ok(())
    }

    #[test]
    fn test_target_connection() -> Result<(), Error> {
        let org = Event::ResponseCheckVersion {
            connection: None,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(EventTarget::Connection, org.target());
        Ok(())
    }

    #[test]
    fn test_event_opcode_some() -> Result<(), Error> {
        let org = Event::ResponseCheckVersion {
            connection: None,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(Some(Opcode::S_CHECK_VERSION), org.opcode());
        Ok(())
    }

    #[test]
    fn test_event_opcode_none() -> Result<(), Error> {
        let (response_channel, _) = channel(1);
        let org = Event::RequestRegisterConnection {
            connection: None,
            response_channel,
        };

        assert_eq!(None, org.opcode());
        Ok(())
    }

    #[test]
    fn test_event_kind() -> Result<(), Error> {
        let org = Event::RequestLoginArbiter {
            connection: None,
            packet: CLoginArbiter {
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                region: Region::Europe,
                patch_version: 0,
            },
        };
        assert_eq!(EventKind::Request, org.kind());
        Ok(())
    }

    #[test]
    fn test_event_connection_some() -> Result<(), Error> {
        let mut world = World::new();
        let entities = world.insert((), vec![(1,)]);
        let entity_id = entities[0];

        let org = Event::ResponseCheckVersion {
            connection: Some(entity_id),
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(Some(entity_id), org.connection());
        Ok(())
    }

    #[test]
    fn test_event_connection_none() -> Result<(), Error> {
        let (response_channel, _) = channel(1);
        let org = Event::RequestRegisterConnection {
            connection: None,
            response_channel,
        };

        assert_eq!(None, org.connection());
        Ok(())
    }
}
