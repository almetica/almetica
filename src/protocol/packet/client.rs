use serde::Deserialize;

#[derive(Deserialize, PartialEq, Debug)]
struct CGetUserGuildLogo {
    player_id: i32,
    guild_id: i32,
}

#[cfg(test)]
mod tests {
    use super::super::super::serde::from_vec;
    use super::*;

    #[test]
    fn test_c_get_user_guild_logo() {
        let data = vec![0x1, 0x2f, 0x31, 0x1, 0x75, 0xe, 0x0, 0x0];
        let expected = CGetUserGuildLogo {
            player_id: 20000513,
            guild_id: 3701,
        };

        assert_eq!(expected, from_vec(data).unwrap());
    }
}
