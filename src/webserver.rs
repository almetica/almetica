/// This modules implements the web server interface.
pub mod request;
pub mod response;

use http_types::StatusCode;
use sqlx::PgPool;
use tide::{Request, Response, Server};
use tracing::{error, info};

use crate::config::Configuration;
use crate::Result;

struct WebServerState {
    config: Configuration,
    pool: PgPool,
}

/// Main loop of the web server.
pub async fn run(pool: PgPool, config: Configuration) -> Result<()> {
    let listen_string = format!("{}:{}", config.server.hostname, config.server.web_port);

    // FIXME: Add a body length limiting middleware once official implemented: https://github.com/http-rs/tide/issues/448

    let mut webserver = Server::with_state(WebServerState { config, pool });
    webserver.middleware(tide::middleware::RequestLogger::new());
    webserver.at("/server/list.uk").get(server_list_endpoint); // FIXME: wildcard!
    webserver.at("/auth").post(auth_endpoint);
    webserver.listen(listen_string).await?;
    Ok(())
}

/// Handles the sever listing
async fn server_list_endpoint(req: Request<WebServerState>) -> Response {
    let server_list_template = format!(
        r###"<serverlist>
<server>
<id>1</id>
<ip>{}</ip>
<port>{}</port>
<category sort="1">Almetica</category>
<name raw_name="Almetica">Almetica</name>
<crowdness sort="1">None</crowdness>
<open sort="1">Recommended</open>
<permission_mask>0x00000000</permission_mask>
<server_stat>0x00000000</server_stat>
<popup> This server isn't up yet! </popup>
<language>en</language>
</server>
</serverlist>"###,
        req.state().config.server.hostname,
        req.state().config.server.game_port
    );

    Response::new(StatusCode::Ok).body_string(server_list_template)
}

/// Handles the client authentication.
async fn auth_endpoint(mut req: Request<WebServerState>) -> Response {
    let login_request: request::Login = match req.body_form().await {
        Ok(login) => login,
        Err(e) => {
            error!("Couldn't deserialize login request: {:?}", e);
            return Response::new(StatusCode::BadRequest);
        }
    };

    // TODO query database and do the login
    // TODO include proper ticket and other fields (chars_per_server and access_level/user_permission etc)
    let _conn = &req.state().pool;

    // TODO when registering, only allow unicode letters (\p{Letter}) and numeric characters (\p{Number})
    let account = login_request.accountname;
    let ticket = "DEADDEADDEADDEAD".to_string();

    info!(
        "Account {} created auth ticket: {}",
        account.clone(),
        ticket.clone()
    );

    let resp = response::AuthResponse {
        last_connected_server_id: 1,
        chars_per_server: vec![],
        account_bits: "0x00000000".to_string(),
        result_message: "OK".to_string(),
        result_code: 200,
        access_level: 1,
        user_permission: 0,
        game_account_name: "TERA".to_string(),
        master_account_name: account,
        ticket,
    };

    match Response::new(StatusCode::Ok).body_json(&resp) {
        Ok(resp) => resp,
        Err(e) => {
            error!("Couldn't serialize auth response: {:?}", e);
            return Response::new(StatusCode::InternalServerError);
        }
    }
}
