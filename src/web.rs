use std::net::SocketAddr;

use tracing::info;
use warp::{Filter, Rejection, Reply};

/// This modules implements the web interface..
pub mod request;
pub mod response;

/// Main loop of the web server.
pub async fn run() {
    let db = "test".to_string();
    let api = auth_filter(db.clone()).or(server_list_filter(db));
    let routes = api.with(warp::log("almetica::web"));

    let listen_addr_string = "127.0.0.1:8080";
    let listen_addr: SocketAddr = listen_addr_string.parse().expect("Unable to parse listen address");
    warp::serve(routes).run(listen_addr).await;
}

// string = db connection
fn with_db(db: String) -> impl Filter<Extract = (String,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

// /server/list.* filter
fn server_list_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    // The TERA client needs to have the region endings (.uk / .de etc.) at the end or else it will not start!
    list_cn_filter(db.clone())
        .or(list_de_filter(db.clone()))
        .or(list_en_filter(db.clone()))
        .or(list_fr_filter(db.clone()))
        .or(list_jp_filter(db.clone()))
        .or(list_kr_filter(db.clone()))
        .or(list_ru_filter(db.clone()))
        .or(list_th_filter(db.clone()))
        .or(list_uk_filter(db))
}

// GET /server/list.cn
fn list_cn_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.cn")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.de
fn list_de_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.de")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.en
fn list_en_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.en")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.fr
fn list_fr_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.fr")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.jp
fn list_jp_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.jp")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.kr
fn list_kr_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.kr")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.ru
fn list_ru_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.ru")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.th
fn list_th_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.th")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// GET /server/list.uk
fn list_uk_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("server" / "list.uk")
        .and(warp::get())
        .and(with_db(db))
        .and_then(server_list_handler)
}

// POST /auth
fn auth_filter(db: String) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    warp::path!("auth")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::form())
        .and(with_db(db))
        .and_then(auth_handler)
}

/// Handles the server listening.
async fn server_list_handler(_db: String) -> Result<impl Reply, Rejection> {
    // TODO include the configuration settings here

    let server_list_template = r###"<serverlist>
<server>
<id>1</id>
<ip>127.0.0.1</ip>
<port>10001</port>
<category sort="1">Almetica</category>
<name raw_name="Almetica">Almetica</name>
<crowdness sort="1">None</crowdness>
<open sort="1">Recommended</open>
<permission_mask>0x00000000</permission_mask>
<server_stat>0x00000000</server_stat>
<popup> This server isn't up yet! </popup>
<language>en</language>
</server>
</serverlist>"###;

    Ok(warp::reply::html(server_list_template))
}

/// Handles the client authentication.
async fn auth_handler(_login: request::Login, _db: String) -> Result<impl Reply, Rejection> {
    // TODO query database and do the login
    // TODO include proper UUID and other fields (chars_per_server and access_level/user_permission etc) "cb3c75d4-66a6-4506-a549-c8ae53fbafd8".to_string()
    let resp = response::AuthResponse {
        last_connected_server_id: 1,
        chars_per_server: vec![],
        account_bits: "0x00000000".to_string(),
        result_message: "OK".to_string(),
        result_code: 200,
        access_level: 1,
        user_permission: 0,
        game_account_name: "TERA".to_string(),
        master_account_name: "Almetica".to_string(),
        ticket: "eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.VFb0qJ1LRg_4ujbZoRMXnVkUgiuKq5KxWqNdbKq_G9Vvz-S1zZa9LPxtHWKa64zDl2ofkT8F6jBt_K4riU-fPg" // HS512 JWT
            .to_string(),
    };

    Ok(warp::reply::json(&resp))
}
