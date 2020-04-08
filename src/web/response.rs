use serde::Serialize;

#[derive(Serialize)]
pub struct ServerCharactersInfo {
    pub id: i32,
    pub char_count: u32,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub last_connected_server_id: i32,
    // 1
    pub chars_per_server: Vec<ServerCharactersInfo>,
    pub account_bits: String, // ??? Possible value: 0x041F000D or 0x00000000?

    #[serde(rename = "result-message")]
    pub result_message: String, // OK

    #[serde(rename = "result-code")]
    pub result_code: i32, // 200

    pub access_level: i32,
    // Normal user = 1
    pub user_permission: i32,
    // Normal user = 0
    pub game_account_name: String,
    // Always "TERA"
    pub master_account_name: String,
    // We will use a UUID here, so that LOGIN and GAME server don't need to expose their indexes for synchronization.
    pub ticket: String, // Can be any string that is ASCII printable. Use some kind of signature so that LOGIN and GAME server don't need a connection to each other.
}
