use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, PartialEq, Debug)]
pub struct CCheckVersion {
    version: Vec<CCheckVersionEntry>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug)]
pub struct CCheckVersionEntry {
    index: i32,
    value: i32,
}

#[derive(Deserialize, Serialize, PartialEq, Debug)]
pub struct CGetUserGuildLogo {
    player_id: i32,
    guild_id: i32,
}

#[derive(Deserialize, Serialize, PartialEq, Debug)]
pub struct CLoginArbiter {
    master_account_name: String,
    #[serde(with = "serde_bytes")]
    ticket: Vec<u8>,

    unk1: i32,
    unk2: u8,
    language: u32, // TODO enum
    patch_version: i32,
}

#[cfg(test)]
mod tests {
    use super::super::super::serde::{from_vec, to_vec, Error};
    use super::*;

    #[test]
    fn test_c_check_version() -> Result<(), Error> {
        let org = vec![
            0x2, 0x0, 0x8, 0x0, 0x8, 0x0, 0x14, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1d, 0x8a, 0x5, 0x0,
            0x14, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0xce, 0x7b, 0x5, 0x0,
        ];
        let data = org.clone();
        let expected = CCheckVersion {
            version: vec![
                CCheckVersionEntry {
                    index: 0,
                    value: 363037,
                },
                CCheckVersionEntry {
                    index: 1,
                    value: 359374,
                },
            ],
        };

        assert_eq!(expected, from_vec(data)?);
        assert_eq!(org, to_vec(expected)?);
        Ok(())
    }

    #[test]
    fn test_c_get_user_guild_logo() -> Result<(), Error> {
        let org = vec![0x1, 0x2f, 0x31, 0x1, 0x75, 0xe, 0x0, 0x0];
        let data = org.clone();
        let expected = CGetUserGuildLogo {
            player_id: 20000513,
            guild_id: 3701,
        };

        assert_eq!(expected, from_vec(data)?);
        assert_eq!(org, to_vec(expected)?);
        Ok(())
    }

    #[test]
    fn test_c_login_arbiter() -> Result<(), Error> {
        let org = vec![
            0x17, 0x0, 0x33, 0x0, 0x32, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x6, 0x0, 0x0, 0x0, 0x2a,
            0x23, 0x0, 0x0, 0x72, 0x0, 0x6f, 0x0, 0x79, 0x0, 0x61, 0x0, 0x6c, 0x0, 0x42, 0x0, 0x75,
            0x0, 0x73, 0x0, 0x68, 0x0, 0x35, 0x0, 0x39, 0x0, 0x31, 0x0, 0x35, 0x0, 0x0, 0x0, 0x4f,
            0x53, 0x63, 0x47, 0x4b, 0x74, 0x6d, 0x72, 0x33, 0x73, 0x6e, 0x67, 0x62, 0x34, 0x31,
            0x38, 0x72, 0x46, 0x6e, 0x48, 0x45, 0x44, 0x57, 0x4d, 0x54, 0x72, 0x59, 0x53, 0x62,
            0x48, 0x61, 0x32, 0x38, 0x30, 0x6a, 0x76, 0x65, 0x5a, 0x74, 0x43, 0x65, 0x47, 0x37,
            0x54, 0x37, 0x70, 0x58, 0x76, 0x37, 0x48,
        ];
        let data = org.clone();
        let expected = CLoginArbiter {
            master_account_name: "royalBush5915".to_string(),
            ticket: vec![
                79, 83, 99, 71, 75, 116, 109, 114, 51, 115, 110, 103, 98, 52, 49, 56, 114, 70, 110,
                72, 69, 68, 87, 77, 84, 114, 89, 83, 98, 72, 97, 50, 56, 48, 106, 118, 101, 90,
                116, 67, 101, 71, 55, 84, 55, 112, 88, 118, 55, 72,
            ],
            unk1: 0,
            unk2: 0,
            language: 6,
            patch_version: 9002,
        };

        assert_eq!(expected, from_vec(data)?);
        assert_eq!(org, to_vec(expected)?);
        Ok(())
    }
}
