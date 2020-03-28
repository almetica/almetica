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

#[cfg(test)]
mod tests {
    use super::super::super::serde::{Error, from_vec, to_vec};
    use super::*;

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
}
