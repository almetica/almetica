use std::collections::HashMap;
use serde::Serialize;
use warp::Filter;

#[derive(Serialize)]
struct ServerCharactersInfo {
    id: i32,
    char_count: u32,
}

#[derive(Serialize)]
struct AuthResponse {
    last_connected_server_id: i32, // 1
    chars_per_server: Vec<ServerCharactersInfo>, // Vec of ServerCharactersInfo
    account_bits: String, // ??? Possible vlaue: 0x041F000D or 0x00000000?

    #[serde(rename = "result-message")] 
    result_message: String, // OK

    #[serde(rename = "result-code")]
    result_code: i32, // 200

    access_level: i32, // Normal user = 1
    user_permission: i32, // Normal user = 0
    game_account_name: String, // always TERA
    master_account_name: String, // user account name?
    ticket: String, // TODO maybe a JWT
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // GET /server
    let server = warp::get()
        .and(warp::path("server"))
        .map(|| r###"<serverlist>
<server>
<id>1</id>
<ip>127.0.0.1</ip>
<port>10001</port>
<category sort="1">Almetica</category>
<name raw_name="Almetica"> Almetica </name>
<crowdness sort="1">None</crowdness>
<open sort="1">Recommended</open>
<permission_mask>0x00000000</permission_mask>
<server_stat>0x00000000</server_stat>
<popup> This server isn't up yet! </popup>
<language>en</language>
</server>
</serverlist>"###);

    // GET /auth
    let auth = warp::post()
        .and(warp::path("auth"))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::form())
        .map(|simple_map: HashMap<String, String>| {
            // TODO proper auth handling
            let resp = AuthResponse {
                last_connected_server_id: 1,
                chars_per_server: vec![ServerCharactersInfo {
                    id: 1,
                    char_count: 1,
                }],
                account_bits: "0x00000000".to_string(),
                result_message: "OK".to_string(),
                result_code: 200,
                access_level: 1,
                user_permission: 0,
                game_account_name: "TERA".to_string(),
                master_account_name: "Almetica".to_string(),
                ticket: "XXXXXXXXXXXXXXXXXX".to_string(),
            };

            warp::reply::json(&resp)
        });

    let log = warp::log("almetica::login");
    let routes = server.or(auth).with(log);

    // TODO configuration file system
    warp::serve(routes)
        // TODO find a sane way to configure this mess
        .run(([127, 0, 0, 1], 8080))
        .await;
}
