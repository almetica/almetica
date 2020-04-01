/// Events that are handled / emitted by the ECS.
///
/// There are Request events, Response events and normal events.
///
/// Request events can be either global or local. Global request events are
/// always send to the global world ECS first by the connection handler.
///
/// Request and Response events need a protocol packet as the first argument.
use std::fmt;

use super::super::protocol::opcode::Opcode;
use super::super::protocol::packet::client::*;
use super::super::protocol::packet::server::*;
use super::super::protocol::serde::{from_vec, to_vec};
use super::super::{Error, Result};

use tokio::sync::mpsc::Sender;

macro_rules! assemble_event {
    (
    Global Request Event {
        $($g_ty:ident{packet: $g_packet_type:ty $(, $g_arg_name:ident: $g_arg_type:ty)*} -> $g_opcode:ident,)*
    }
    Local Request Event {
        $($l_ty:ident{packet: $l_packet_type:ty $(, $l_arg_name:ident: $l_arg_type:ty)*} -> $l_opcode:ident,)*
    }
    Response Event {
        $($r_ty:ident{packet: $r_packet_type:ty $(, $r_arg_name:ident: $r_arg_type:ty)*} -> $r_opcode:ident,)*
    }
    Event {
        $($e_ty:ident{$($e_arg_name:ident: $e_arg_type:ty)*},)*
    }
    ) => {
        /// Event enum for all events.
        #[derive(Clone, Debug)]
        pub enum Event {
            $($g_ty {packet: $g_packet_type $(,$g_arg_name: $g_arg_type)*},)*
            $($l_ty {packet: $l_packet_type $(,$l_arg_name: $l_arg_type)*},)*
            $($r_ty {packet: $r_packet_type $(,$r_arg_name: $r_arg_type)*},)*
            $($e_ty {$($e_arg_name: $e_arg_type),*},)*
        }

        impl Event {
            /// Creates a new Request/Response event for the given opcode & packet data.
            pub fn new_from_packet(opcode: Opcode, packet_data: Vec<u8>) -> Result<Event> {
                match opcode {
                    $(Opcode::$g_opcode => {
                        let p = from_vec(packet_data)?;
                        Ok(Event::$g_ty{ packet: p })
                    },)*
                    $(Opcode::$l_opcode => {
                        let p = from_vec(packet_data)?;
                        Ok(Event::$l_ty{ packet: p })
                    },)*
                    $(Opcode::$r_opcode => {
                        let p = from_vec(packet_data)?;
                        Ok(Event::$r_ty{ packet: p })
                    },)*
                    _ => Err(Error::NoEventMappingForPacket),
                }
            }

            /// Get the data from a Response event. None if a Request event or normal event.
            pub fn get_data(&self) -> Result<Option<Vec<u8>>> {
                match self {
                    $(Event::$r_ty{packet $(,$r_arg_name)*} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    _ => Ok(None),
                }
            }

            /// Get the opcode from a Response event. None if a Request event or normal event.
            #[allow(unused_variables)]
            pub fn get_opcode(&self) -> Option<Opcode> {
                match self {
                    $(Event::$r_ty{packet $(,$r_arg_name)*} => {
                        Some(Opcode::$r_opcode)
                    },)*
                    _ => None,
                }
            }

            /// Returns true if the event is an event that needs to be send to the global world ECS.
            #[allow(unused_variables)]
            pub fn is_global(&self) -> bool {
                match self {
                    $(Event::$l_ty{packet $(,$l_arg_name)*} => false,)*
                    $(Event::$g_ty{packet $(,$g_arg_name)*} => true,)*
                    _ => false,
                }
            }
        }

        impl fmt::Display for Event {
            #[allow(unused_variables)]
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(Event::$g_ty{packet $(,$g_arg_name)*} => write!(f, "{}", stringify!($g_ty)),)*
                    $(Event::$l_ty{packet $(,$l_arg_name)*} => write!(f, "{}", stringify!($l_ty)),)*
                    $(Event::$r_ty{packet $(,$r_arg_name)*} => write!(f, "{}", stringify!($r_ty)),)*
                    $(Event::$e_ty{$($e_arg_name,)*} => write!(f, "{}", stringify!($e_ty)),)*
                }

            }
        }
    };
}

// Request and Response events need to have a packet as it's first argument and an Opcode mapping.
assemble_event! {
    Global Request Event {
        RequestLoginArbiter{packet: CLoginArbiter} -> C_LOGIN_ARBITER,
        RequestCheckVersion{packet: CCheckVersion} -> C_CHECK_VERSION,
    }
    Local Request Event {
    }
    Response Event {
        ResponseCheckVersion{packet: SCheckVersion} -> S_CHECK_VERSION,
    }
    Event {
        // Registers the response channel of a connection at a world.
        RegisterConnection{response_channel: Sender<Box<Event>>},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Region;
    use crate::protocol::opcode::Opcode;
    use crate::Error;
    use tokio::sync::mpsc::channel;

    #[test]
    fn test_opcode_mapping() -> Result<(), Error> {
        let data = vec![
            0x2, 0x0, 0x8, 0x0, 0x8, 0x0, 0x14, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1d, 0x8a, 0x5, 0x0, 0x14, 0x0, 0x0, 0x0,
            0x1, 0x0, 0x0, 0x0, 0xce, 0x7b, 0x5, 0x0,
        ];
        let event = Event::new_from_packet(Opcode::C_CHECK_VERSION, data)?;
        if let Event::RequestCheckVersion { packet } = event {
            assert_eq!(0, packet.version[0].index);
            assert_eq!(363037, packet.version[0].value);
            assert_eq!(1, packet.version[1].index);
            assert_eq!(359374, packet.version[1].value);
        } else {
            panic!("New didn't returned the right Event.");
        }
        Ok(())
    }

    #[test]
    fn test_is_global() -> Result<(), Error> {
        let org = Event::RequestLoginArbiter {
            packet: CLoginArbiter {
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                region: Region::Europe,
                patch_version: 0,
            },
        };
        assert_eq!(true, org.is_global());
        Ok(())
    }

    #[test]
    fn test_get_opcode_some() -> Result<(), Error> {
        let org = Event::ResponseCheckVersion {
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(Some(Opcode::S_CHECK_VERSION), org.get_opcode());
        Ok(())
    }

    #[test]
    fn test_get_opcode_none() -> Result<(), Error> {
        let (response_channel, _) = channel(1);
        let org = Event::RegisterConnection { response_channel };

        assert_eq!(None, org.get_opcode());
        Ok(())
    }
}
