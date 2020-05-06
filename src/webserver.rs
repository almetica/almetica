/// This modules implements the web server interface.
pub mod request;
pub mod response;

use anyhow::ensure;
use async_std::task;
use http_types::StatusCode;
use sqlx::PgPool;
use tide::{Request, Response, Server};
use tracing::{error, info};

use crate::config::Configuration;
use crate::crypt::password_hash::verify_hash;
use crate::model::repository::{account, loginticket};
use crate::model::PasswordHashAlgorithm;
use crate::{AlmeticaError, Result};

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
    webserver.at("/server/*").get(server_list_endpoint);
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
<category sort="1">PVE</category>
<name raw_name="Almetica">Almetica</name>
<crowdness sort="1">None</crowdness>
<open sort="1">Recommended</open>
<permission_mask>0x00000000</permission_mask>
<server_stat>0x00000000</server_stat>
<popup>This server isn't up yet!</popup>
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

    let pool = &req.state().pool;
    let account_name = login_request.accountname;
    let password = login_request.password;

    let ticket: String = match login(pool, account_name.clone(), password).await {
        Ok(token) => token,
        Err(e) => {
            return match e.downcast_ref::<AlmeticaError>() {
                Some(AlmeticaError::InvalidLogin) => {
                    info!("Invalid login for account {}", account_name);
                    invalid_login_response(StatusCode::Unauthorized, account_name)
                }
                Some(..) | None => {
                    error!("Can't verify login: {}", e);
                    invalid_login_response(StatusCode::InternalServerError, account_name)
                }
            };
        }
    };

    info!(
        "Account {} created auth ticket: {}",
        account_name.clone(),
        ticket
    );

    valid_login_response(account_name, ticket)
}

/// Tries to login with the given credentials. Returns the login ticket if successful.
async fn login(pool: &PgPool, account_name: String, password: String) -> Result<String> {
    let mut conn = pool.acquire().await?;
    let (account_id, password_hash, password_algorithm) =
        match account::get_by_name(&mut conn, &account_name).await {
            Ok(acc) => (acc.id, acc.password, acc.algorithm),
            Err(..) => (
                0,
                "dummy_hash_for_constant_time_operation".to_string(),
                PasswordHashAlgorithm::Argon2,
            ),
        };

    let is_valid = task::spawn_blocking(move || {
        verify_hash(password.as_bytes(), &password_hash, password_algorithm)
    })
    .await?;
    ensure!(is_valid, AlmeticaError::InvalidLogin);

    let ticket = loginticket::upsert_ticket(&mut conn, account_id).await?;
    Ok(ticket.ticket)
}

// TODO chars per server once user entity is implemented
fn valid_login_response(account_name: String, ticket: String) -> Response {
    let resp = response::AuthResponse {
        last_connected_server_id: 1,
        chars_per_server: vec![],
        account_bits: "0x00000000".to_string(),
        result_message: "OK".to_string(),
        result_code: 200,
        access_level: 1,
        user_permission: 0,
        game_account_name: "TERA".to_string(),
        master_account_name: account_name,
        ticket,
    };

    create_response(&resp, StatusCode::Ok)
}

fn invalid_login_response(status: StatusCode, account_name: String) -> Response {
    let resp = response::AuthResponse {
        last_connected_server_id: 0,
        chars_per_server: vec![],
        account_bits: "0x00000000".to_string(),
        result_message: status.canonical_reason().to_string(),
        result_code: status as i32,
        access_level: 0,
        user_permission: 0,
        game_account_name: "TERA".to_string(),
        master_account_name: account_name,
        ticket: "".to_string(),
    };

    create_response(&resp, StatusCode::InternalServerError)
}

fn create_response(resp: &response::AuthResponse, status_code: StatusCode) -> Response {
    match Response::new(status_code).body_json(&resp) {
        Ok(resp) => resp,
        Err(e) => {
            error!("Couldn't serialize auth response: {:?}", e);
            Response::new(StatusCode::InternalServerError)
        }
    }
}
