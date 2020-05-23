/// Module for client network packages.
use crate::model::{Class, Customization, Gender, Race, Region};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CCanCreateUser {}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CChangeUserLobbySlotId {
    pub user_positions: Vec<CChangeUserLobbySlotIdEntry>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CChangeUserLobbySlotIdEntry {
    pub database_id: i32,
    pub lobby_slot: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CCheckVersion {
    pub version: Vec<CCheckVersionEntry>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CCheckVersionEntry {
    pub index: i32,
    pub value: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CCheckUserName {
    pub name: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CCreateUser {
    pub name: String,
    #[serde(with = "serde_bytes")]
    pub details: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub shape: Vec<u8>,
    pub gender: Gender,
    pub race: Race,
    pub class: Class,
    pub appearance: Customization,
    pub is_second_character: bool, // Unused
    pub appearance2: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CDeleteUser {
    pub database_id: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CGetUserList {}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CGetUserGuildLogo {
    pub player_id: i32,
    pub guild_id: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CLoadTopoFin {}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CLoginArbiter {
    pub master_account_name: String,
    #[serde(with = "serde_bytes")]
    pub ticket: Vec<u8>,
    pub unk1: i32,
    pub unk2: u8,
    pub region: Region,
    pub patch_version: i32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CPong {}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CSelectUser {
    pub database_id: i32,
    pub unk1: u8,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct CSetVisibleRange {
    pub range: u32,
}

#[cfg(test)]
#[macro_use]
mod tests {
    use crate::model::{Class, Customization, Gender, Race, Region};
    use crate::protocol::serde::{from_vec, to_vec, Result};

    use super::*;

    packet_test!(
        name: test_can_create_user,
        data: vec![],
        expected: CCanCreateUser {}
    );

    packet_test!(
        name: test_change_user_lobby_slot_id,
        data: vec![2, 0, 8, 0, 8, 0, 20, 0, 5, 0, 0, 0, 1, 0, 0, 0, 20, 0, 0, 0, 6, 0, 0, 0, 2, 0, 0, 0],
        expected: CChangeUserLobbySlotId {
            user_positions: vec![
                CChangeUserLobbySlotIdEntry {
                    database_id: 5,
                    lobby_slot: 1,
                },
                CChangeUserLobbySlotIdEntry {
                    database_id: 6,
                    lobby_slot: 2,
                },
            ],
        }
    );

    packet_test!(
        name: test_check_version,
        data: vec![
            0x2, 0x0, 0x8, 0x0, 0x8, 0x0, 0x14, 0x0, 0x0, 0x0, 0x0, 0x0, 0x8e, 0x96, 0x5, 0x0, 0x14, 0x0, 0x0, 0x0,
            0x1, 0x0, 0x0, 0x0, 0xdf, 0x93, 0x5, 0x0,
        ],
        expected: CCheckVersion {
            version: vec![
                CCheckVersionEntry {
                    index: 0,
                    value: 366_222,
                },
                CCheckVersionEntry {
                    index: 1,
                    value: 365_535,
                },
            ],
        }
    );

    packet_test!(
        name: test_check_user_name,
        data: vec![
            0x6, 0x0, 0x54, 0x0, 0x68, 0x0, 0x65, 0x0, 0x42, 0x0, 0x65, 0x0, 0x73, 0x0,
            0x74, 0x0, 0x4e, 0x0, 0x61, 0x0, 0x6d, 0x0, 0x65, 0x0, 0x0, 0x0,
        ],
        expected: CCheckUserName {
            name: "TheBestName".to_string(),
        }
    );

    packet_test!(
        name: test_create_user,
        data: vec![
            0x27, 0x0, 0x33, 0x0, 0x20, 0x0, 0x53, 0x0, 0x40, 0x0, 0x1, 0x0, 0x0, 0x0, 0x4, 0x0,
            0x0, 0x0, 0xc, 0x0, 0x0, 0x0, 0x65, 0x1e, 0xb, 0x1, 0x9, 0x19, 0x4, 0x0, 0x0, 0x64,
            0x0, 0x0, 0x0, 0x41, 0x0, 0x73, 0x0, 0x75, 0x0, 0x6e, 0x0, 0x61, 0x0, 0x0, 0x0, 0xd,
            0x13, 0x1a, 0x8, 0x0, 0x0, 0x0, 0x0, 0x1f, 0xa, 0x4, 0x0, 0x17, 0xa, 0x0, 0x0, 0x9,
            0x0, 0xc, 0xd, 0x0, 0x0, 0x0, 0x0, 0x15, 0x1f, 0xe, 0x16, 0x1d, 0x10, 0x10, 0x0, 0x1,
            0x13, 0x10, 0x13, 0x13, 0x10, 0x13, 0x13, 0x13, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf,
            0x10, 0x13, 0xa, 0x0, 0x5, 0xb, 0x10, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
        ],
        expected: CCreateUser {
            name: "Asuna".to_string(),
            details: vec![
                13, 19, 26, 8, 0, 0, 0, 0, 31, 10, 4, 0, 23, 10, 0, 0, 9, 0, 12, 13, 0, 0, 0, 0,
                21, 31, 14, 22, 29, 16, 16, 0,
            ],
            shape: vec![
                1, 19, 16, 19, 19, 16, 19, 19, 19, 15, 15, 15, 15, 15, 15, 15, 16, 19, 10, 0, 5,
                11, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            gender: Gender::Female,
            race: Race::ElinPopori,
            class: Class::Valkyrie,
            appearance: Customization(vec![101, 30, 11, 1, 9, 25, 4, 0]),
            is_second_character: false,
            appearance2: 100,
        }
    );

    packet_test!(
        name: test_delete_user,
        data: vec![0x13, 0x12, 0x11, 0x32],
        expected: CDeleteUser {
            database_id: 839979539,
        }
    );

    packet_test!(
        name: test_get_user_guild_logo,
        data: vec![0x1, 0x2f, 0x31, 0x1, 0x75, 0xe, 0x0, 0x0],
        expected: CGetUserGuildLogo {
            player_id: 20_000_513,
            guild_id: 3701,
        }
    );

    packet_test!(
        name: test_get_user_list,
        data: vec![],
        expected: CGetUserList {}
    );

    packet_test!(
        name: test_load_topo_fin,
        data: vec![],
        expected: CLoadTopoFin {}
    );

    packet_test!(
        name: test_login_arbiter,
        data: vec![
            0x17, 0x0, 0x33, 0x0, 0x32, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x6, 0x0, 0x0, 0x0, 0x2a,
            0x23, 0x0, 0x0, 0x72, 0x0, 0x6f, 0x0, 0x79, 0x0, 0x61, 0x0, 0x6c, 0x0, 0x42, 0x0, 0x75,
            0x0, 0x73, 0x0, 0x68, 0x0, 0x35, 0x0, 0x39, 0x0, 0x31, 0x0, 0x35, 0x0, 0x0, 0x0, 0x4f,
            0x53, 0x63, 0x47, 0x4b, 0x74, 0x6d, 0x72, 0x33, 0x73, 0x6e, 0x67, 0x62, 0x34, 0x31,
            0x38, 0x72, 0x46, 0x6e, 0x48, 0x45, 0x44, 0x57, 0x4d, 0x54, 0x72, 0x59, 0x53, 0x62,
            0x48, 0x61, 0x32, 0x38, 0x30, 0x6a, 0x76, 0x65, 0x5a, 0x74, 0x43, 0x65, 0x47, 0x37,
            0x54, 0x37, 0x70, 0x58, 0x76, 0x37, 0x48,
        ],
        expected: CLoginArbiter {
            master_account_name: "royalBush5915".to_string(),
            ticket: vec![
                79, 83, 99, 71, 75, 116, 109, 114, 51, 115, 110, 103, 98, 52, 49, 56, 114, 70, 110,
                72, 69, 68, 87, 77, 84, 114, 89, 83, 98, 72, 97, 50, 56, 48, 106, 118, 101, 90,
                116, 67, 101, 71, 55, 84, 55, 112, 88, 118, 55, 72,
            ],
            unk1: 0,
            unk2: 0,
            region: Region::Europe,
            patch_version: 9002,
        }
    );

    packet_test!(
        name: test_pong,
        data: vec![],
        expected: CPong {}
    );

    packet_test!(
        name: test_select_user,
        data: vec![0x3, 0x2f, 0x32, 0x1, 0x0],
        expected: CSelectUser {
            database_id: 20066051,
            unk1: 0,
        }
    );

    packet_test!(
        name: test_set_visible_range,
        data: vec![0xd0, 0x7, 0x0, 0x0],
        expected: CSetVisibleRange {
            range: 2000,
        }
    );
}
