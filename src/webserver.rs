/// This modules implements the web server interface.
pub mod request;
pub mod response;
use crate::config::Configuration;
use crate::crypt::password_hash::verify_hash;
use crate::model::repository::{account, loginticket};
use crate::model::PasswordHashAlgorithm;
use crate::webserver::response::{AuthResponse, ServerListEntry, ServerListResponse};
use crate::{AlmeticaError, Result};
use anyhow::ensure;
use async_std::task;
use http_types::StatusCode;
use serde::Serialize;
use sqlx::PgPool;
use tide::{Request, Response, Server};
use tracing::{error, info};

struct WebServerState {
    config: Configuration,
    pool: PgPool,
}

/// Main loop of the web server.
pub async fn run(pool: PgPool, config: Configuration) -> Result<()> {
    let listen_string = format!("{}:{}", config.server.ip, config.server.web_port);

    // FIXME: Add a body length limiting middleware once official implemented: https://github.com/http-rs/tide/issues/448

    let mut webserver = Server::with_state(WebServerState { config, pool });
    webserver.at("/server/*").get(server_list_endpoint);
    webserver.at("/auth").post(auth_endpoint);
    webserver.listen(listen_string).await?;
    Ok(())
}

/// Handles the server listing
async fn server_list_endpoint(req: Request<WebServerState>) -> tide::Result<Response> {
    let category = if req.state().config.game.pvp {
        "PVP"
    } else {
        "PVE"
    };

    let server_list = ServerListResponse {
        // TODO make the name and raw_name configurable
        servers: vec![ServerListEntry {
            id: 1,
            category: category.to_string(),
            raw_name: "Almetica".to_string(),
            name: "Almetica".to_string(),
            crowdness: "None".to_string(),
            open: "Recommended".to_string(),
            ip: req.state().config.server.ip,
            port: req.state().config.server.game_port,
            lang: 1,
            popup: "This server isn't up yet!".to_string(),
        }],
    };

    Ok(create_response(&server_list, StatusCode::Ok))
}

/// Handles the client authentication.
async fn auth_endpoint(mut req: Request<WebServerState>) -> tide::Result<Response> {
    let login_request: request::Login = match req.body_form().await {
        Ok(login) => login,
        Err(e) => {
            error!("Couldn't deserialize login request: {:?}", e);
            return Ok(Response::new(StatusCode::BadRequest));
        }
    };

    let pool = &req.state().pool;
    let account_name = login_request.accountname;
    let password = login_request.password;

    let ticket = match login(pool, &account_name, password).await {
        Ok(token) => token,
        Err(e) => {
            return match e.downcast_ref::<AlmeticaError>() {
                Some(AlmeticaError::InvalidLogin) => {
                    info!("Invalid login for account {}", account_name);
                    Ok(invalid_login_response(StatusCode::Unauthorized))
                }
                Some(..) | None => {
                    error!("Can't verify login: {}", e);
                    Ok(invalid_login_response(StatusCode::InternalServerError))
                }
            };
        }
    };

    info!("Account {} created an auth ticket", account_name);

    Ok(valid_login_response(ticket))
}

// TODO write a test for the login() function
/// Tries to login with the given credentials. Returns the login ticket if successful.
async fn login(pool: &PgPool, account_name: &str, password: String) -> Result<Vec<u8>> {
    let mut conn = pool.acquire().await?;
    let (account_id, password_hash, password_algorithm) =
        match account::get_by_name(&mut conn, account_name).await {
            Ok(acc) => (Some(acc.id), acc.password, acc.algorithm),
            Err(..) => (
                None,
                "$argon2id$v=19$m=131072,t=3,p=8$SFuUVFwwNhz0eLHkBCJmHA$ecyaOGtvgPVEb2ZkmA1z/72q7+kgkwOZeR3VO2V1LnU".to_string(),
                PasswordHashAlgorithm::Argon2,
            ),
        };

    let is_valid = task::spawn_blocking(move || {
        verify_hash(password.as_bytes(), &password_hash, password_algorithm)
    })
    .await?;
    ensure!(account_id.is_some(), AlmeticaError::InvalidLogin);
    ensure!(is_valid, AlmeticaError::InvalidLogin);

    let ticket = loginticket::upsert_ticket(&mut conn, account_id.unwrap()).await?;
    Ok(ticket.ticket)
}

fn create_response(resp: &impl Serialize, status_code: StatusCode) -> Response {
    match Response::new(status_code).body_json(resp) {
        Ok(resp) => resp,
        Err(e) => {
            error!("Couldn't serialize auth response: {:?}", e);
            Response::new(StatusCode::InternalServerError)
        }
    }
}

fn invalid_login_response(status_code: StatusCode) -> Response {
    let auth_resp = AuthResponse {
        ticket: "".to_string(),
    };
    create_response(&auth_resp, status_code)
}

fn valid_login_response(ticket: Vec<u8>) -> Response {
    let encoded_ticket = base64::encode(ticket);
    let auth_resp = AuthResponse {
        ticket: encoded_ticket,
    };
    create_response(&auth_resp, StatusCode::Ok)
}
