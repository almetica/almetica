/// Messages that are handled / emitted by the ECS.
///
/// Messages are only to be used for communication between
/// ECS and connections (and ECS to ECS). They are not to be used for ECS internal communication.
/// For this, you should use other components.
///
/// There are packet and system messages.
/// System Messages are special messages that don't send out a packet to the client and are normally
/// handling the state between the Local ECS, Global ECS or a connection.
///
/// A message always has a target: Global ECS, local ECS or a connection.
///
/// Messages that are coming FROM the client are Requests.
/// Messages that are going TO the client are Responses.
///
/// Network connections and ECS have async ```mpmc``` channels to write messages into.
///
use crate::ecs::dto::UserInitializer;
use crate::protocol::opcode::Opcode;
use crate::protocol::packet::*;
use crate::protocol::serde::{from_vec, to_vec};
use crate::{AlmeticaError, Result};
use anyhow::bail;
use async_std::sync::Sender;
use shipyard::*;
use std::fmt;

/// ECS messages. We use `Box` so that we don't need to copy the packet data around.
pub type EcsMessage = Box<Message>;

/// The target of the message.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MessageTarget {
    GlobalLocal, // Both Global and Local
    Global,
    Local,
    Connection,
}

macro_rules! assemble_message {
    (
    Local Packet Messages {
        $($l_ty:ident{packet: $l_packet_type:ty}, $l_opcode:ident, $l_target:ident;)*
    }
    Global User Packet Messages {
        $($u_ty:ident{packet: $u_packet_type:ty}, $u_opcode:ident, $u_target:ident;)*
    }
    Global Account Packet Messages {
        $($a_ty:ident{packet: $a_packet_type:ty}, $a_opcode:ident, $a_target:ident;)*
    }
    Global Packet Messages {
        $($p_ty:ident{packet: $p_packet_type:ty}, $p_opcode:ident, $p_target:ident;)*
    }
    Special Messages {
        $($s_ty:ident{$($s_arg_name:ident: $s_arg_type:ty),+}, $s_target:ident;)*
    }
    ) => {
        /// Message enum for all messages.
        #[derive(Clone, Debug)]
        pub enum Message {
            $($l_ty {connection_global_world_id: EntityId, connection_local_world_id: EntityId, packet: $l_packet_type},)*
            $($u_ty {connection_global_world_id: EntityId, account_id: i64, user_id: i32, packet: $u_packet_type},)*
            $($a_ty {connection_global_world_id: EntityId, account_id: i64, packet: $a_packet_type},)*
            $($p_ty {connection_global_world_id: EntityId, packet: $p_packet_type},)*
            $($s_ty {$($s_arg_name: $s_arg_type),*},)*
        }

        impl Message {
            /// Creates a new packet message for the given opcode & packet data from a client.
            pub fn new_from_packet(connection_global_world_id: EntityId, connection_local_world_id: Option<EntityId>, account_id: Option<i64>, user_id: Option<i32>, opcode: Opcode, packet_data: Vec<u8>) -> Result<Message> {
                match opcode {
                    $(Opcode::$l_opcode => {
                        if connection_local_world_id.is_none() {
                            bail!(AlmeticaError::UnauthorizedPacket);
                        }

                        let packet = from_vec(packet_data)?;
                        Ok(Message::$l_ty{connection_global_world_id, connection_local_world_id: connection_local_world_id.unwrap(), packet})
                    },)*
                    $(Opcode::$u_opcode => {
                        if user_id.is_none() || account_id.is_none() {
                            bail!(AlmeticaError::UnauthorizedPacket);
                        }

                        let packet = from_vec(packet_data)?;
                        Ok(Message::$u_ty{connection_global_world_id, account_id: account_id.unwrap(), user_id: user_id.unwrap(), packet})
                    },)*
                    $(Opcode::$a_opcode => {
                        if account_id.is_none() {
                            bail!(AlmeticaError::UnauthorizedPacket);
                        }

                        let packet = from_vec(packet_data)?;
                        Ok(Message::$a_ty{connection_global_world_id, account_id: account_id.unwrap(), packet})
                    },)*
                    $(Opcode::$p_opcode => {
                        let packet = from_vec(packet_data)?;
                        Ok(Message::$p_ty{connection_global_world_id: connection_global_world_id, packet})
                    },)*
                    _ => bail!(AlmeticaError::NoMessageMappingForPacket),
                }
            }

            /// Get the connection_id of a packet message.
            pub fn connection_id(&self) -> Option<EntityId> {
                match self {
                    $(Message::$l_ty{connection_local_world_id,..} => Some(*connection_local_world_id),)*
                    $(Message::$u_ty{connection_global_world_id,..} => Some(*connection_global_world_id),)*
                    $(Message::$a_ty{connection_global_world_id,..} => Some(*connection_global_world_id),)*
                    $(Message::$p_ty{connection_global_world_id,..} => Some(*connection_global_world_id),)*
                    $(Message::$s_ty{..} => None,)*
                }
            }

            /// Get the data from a packet message.
            pub fn data(&self) -> Result<Option<Vec<u8>>> {
                match self {
                    $(Message::$l_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    $(Message::$u_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    $(Message::$a_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    $(Message::$p_ty{packet, ..} => {
                        let data = to_vec(packet)?;
                        Ok(Some(data))
                    },)*
                    _ => Ok(None),
                }
            }

            /// Get the opcode from a packet message.
            pub fn opcode(&self) -> Option<Opcode> {
                match self {
                    $(Message::$l_ty{..} => {
                        Some(Opcode::$l_opcode)
                    },)*
                    $(Message::$u_ty{..} => {
                        Some(Opcode::$u_opcode)
                    },)*
                    $(Message::$a_ty{..} => {
                        Some(Opcode::$a_opcode)
                    },)*
                    $(Message::$p_ty{..} => {
                        Some(Opcode::$p_opcode)
                    },)*
                    _ => None,
                }
            }

            /// Get the target of the message (global world / local world / connection).
            pub fn target(&self) -> MessageTarget {
                match self {
                    $(Message::$l_ty{..} => MessageTarget::$l_target,)*
                    $(Message::$u_ty{..} => MessageTarget::$u_target,)*
                    $(Message::$a_ty{..} => MessageTarget::$a_target,)*
                    $(Message::$p_ty{..} => MessageTarget::$p_target,)*
                    $(Message::$s_ty{..} => MessageTarget::$s_target,)*
                }
            }
        }

        impl fmt::Display for Message {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(Message::$l_ty{..} => write!(f, "Message::{}", stringify!($l_ty)),)*
                    $(Message::$u_ty{..} => write!(f, "Message::{}", stringify!($u_ty)),)*
                    $(Message::$a_ty{..} => write!(f, "Message::{}", stringify!($a_ty)),)*
                    $(Message::$p_ty{..} => write!(f, "Message::{}", stringify!($p_ty)),)*
                    $(Message::$s_ty{..} => write!(f, "Message::{}", stringify!($s_ty)),)*
                }
            }
        }
    };
}

assemble_message! {
    // Local packet messages (handled by the LOCAL_WORLD)
    Local Packet Messages {
        RequestLoadTopoFin{packet: CLoadTopoFin}, C_LOAD_TOPO_FIN, Local;
        ResponseSpawnMe{packet: SSpawnMe}, S_SPAWN_ME, Connection;
    }
    // Global packets that need an account ID and the user ID attached.
    Global User Packet Messages {
        ResponseLogin{packet: SLogin}, S_LOGIN, Connection;
    }
    // Global packets that need an account ID attached.
    Global Account Packet Messages {
        RequestCanCreateUser{packet: CCanCreateUser}, C_CAN_CREATE_USER, Global;
        RequestChangeUserLobbySlotId{packet: CChangeUserLobbySlotId}, C_CHANGE_USER_LOBBY_SLOT_ID, Global;
        RequestCheckUserName{packet: CCheckUserName}, C_CHECK_USERNAME, Global;
        RequestCreateUser{packet: CCreateUser}, C_CREATE_USER, Global;
        RequestDeleteUser{packet: CDeleteUser}, C_DELETE_USER, Global;
        RequestGetUserList{packet: CGetUserList}, C_GET_USER_LIST, Global;
        RequestSetVisibleRange{packet: CSetVisibleRange}, C_SET_VISIBLE_RANGE, Global;
        RequestSelectUser{packet: CSelectUser}, C_SELECT_USER, Global;
        ResponseLoginArbiter{packet: SLoginArbiter}, S_LOGIN_ARBITER, Connection;
    }
    // Global packet messages (handled by the GLOBAL_WORLD)
    Global Packet Messages {
        RequestLoginArbiter{packet: CLoginArbiter}, C_LOGIN_ARBITER, Global;
        RequestCheckVersion{packet: CCheckVersion}, C_CHECK_VERSION, Global;
        RequestPong{packet: CPong}, C_PONG, Global;
        ResponseCanCreateUser{packet: SCanCreateUser}, S_CAN_CREATE_USER, Connection;
        ResponseCheckUserName{packet: SCheckUserName}, S_CHECK_USERNAME, Connection;
        ResponseCheckVersion{packet: SCheckVersion}, S_CHECK_VERSION, Connection;
        ResponseCreateUser{packet: SCreateUser}, S_CREATE_USER, Connection;
        ResponseDeleteUser{packet: SDeleteUser}, S_DELETE_USER, Connection;
        ResponseGetUserList{packet: SGetUserList}, S_GET_USER_LIST, Connection;
        ResponseLoadHint{packet: SLoadHint}, S_LOAD_HINT, Connection;
        ResponseLoadTopo{packet: SLoadTopo}, S_LOAD_TOPO, Connection;
        ResponseLoadingScreenControlInfo{packet: SLoadingScreenControlInfo}, S_LOADING_SCREEN_CONTROL_INFO, Connection;
        ResponseLoginAccountInfo{packet: SLoginAccountInfo}, S_LOGIN_ACCOUNT_INFO, Connection;
        ResponsePing{packet: SPing}, S_PING, Connection;
        ResponseRemainPlayTime{packet: SRemainPlayTime}, S_REMAIN_PLAY_TIME, Connection;
    }
    // Special messages send between the global and local world and also the connections.
    Special Messages {
        // Signals an ECS to shut down.
        ShutdownSignal{forced: bool}, GlobalLocal;

        // The connection will be dropped after it receives this message.
        DropConnection{connection_global_world_id: EntityId}, Connection;

        // Registers the connection to the global world.
        RegisterConnection{connection_channel: Sender<EcsMessage>}, Global;

        // The connections get it's EntityId of the global world returned.
        RegisterConnectionFinished{connection_global_world_id: EntityId}, Connection;

        // Connects the connection to a local world.
        RegisterLocalWorld{connection_local_world_id: EntityId, local_world_channel: Sender<EcsMessage>}, Connection;

        // Messages used in the spawn process between the global and local world.
        LocalWorldLoaded{successful: bool, global_world_id: EntityId}, Global;
        PrepareUserSpawn{user_initializer: UserInitializer}, Local;
        UserSpawnPrepared{connection_global_world_id: EntityId, connection_local_world_id: EntityId}, Global;
        UserReadyToConnect{connection_local_world_id: EntityId}, Local;
        UserSpawned{connection_global_world_id: EntityId}, Global;

        // Messages used in the de-spawn process between the global and local world.
        UserDespawn{connection_local_world_id: EntityId}, Local;
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
        let message =
            Message::new_from_packet(entity, None, None, None, Opcode::C_CHECK_VERSION, data)?;
        if let Message::RequestCheckVersion {
            connection_global_world_id: entity_id,
            packet,
        } = message
        {
            assert_eq!(entity, entity_id);
            assert_eq!(packet.version[0].index, 0);
            assert_eq!(packet.version[0].value, 363_037);
            assert_eq!(packet.version[1].index, 1);
            assert_eq!(packet.version[1].value, 359_374);
        } else {
            panic!("New didn't returned the right message.");
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

        match Message::new_from_packet(entity, None, None, None, Opcode::C_CHECK_USERNAME, data) {
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
        let org = Message::RequestLoginArbiter {
            connection_global_world_id: entity,
            packet: CLoginArbiter {
                master_account_name: "test".to_string(),
                ticket: vec![],
                unk1: 0,
                unk2: 0,
                region: Region::Europe,
                patch_version: 0,
            },
        };
        assert_eq!(org.target(), MessageTarget::Global);
        Ok(())
    }

    #[test]
    fn test_target_connection() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Message::ResponseCheckVersion {
            connection_global_world_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.target(), MessageTarget::Connection);
        Ok(())
    }

    #[test]
    fn test_message_opcode_some() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Message::ResponseCheckVersion {
            connection_global_world_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.opcode(), Some(Opcode::S_CHECK_VERSION));
        Ok(())
    }

    #[test]
    fn test_message_opcode_none() -> Result<()> {
        let (connection_channel, _) = channel(1);
        let org = Message::RegisterConnection { connection_channel };

        assert_eq!(org.opcode(), None);
        Ok(())
    }

    #[test]
    fn test_message_connection_id_some() -> Result<()> {
        let entity = World::new().borrow::<EntitiesViewMut>().add_entity((), ());
        let org = Message::ResponseCheckVersion {
            connection_global_world_id: entity,
            packet: SCheckVersion { ok: true },
        };
        assert_eq!(org.connection_id(), Some(entity));
        Ok(())
    }

    #[test]
    fn test_message_register_connection_connection_id_should_panic() {
        let (connection_channel, _) = channel(1);
        let org = Message::RegisterConnection { connection_channel };

        assert_eq!(org.connection_id(), None);
    }
}
