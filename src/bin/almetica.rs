#![warn(clippy::all)]
use almetica::config::{read_configuration, Configuration};
use almetica::crypt::password_hash;
use almetica::dataloader::load_opcode_mapping;
use almetica::ecs::message::EcsMessage;
use almetica::ecs::world::GlobalWorld;
use almetica::model::entity::Account;
use almetica::model::migrations;
use almetica::model::repository::account;
use almetica::model::PasswordHashAlgorithm;
use almetica::networkserver;
use almetica::protocol::opcode::Opcode;
use almetica::webserver;
use almetica::Result;
use anyhow::{bail, Context};
use async_macros::join;
use async_std::sync::Sender;
use async_std::task::{self, JoinHandle};
use chrono::Utc;
use clap::{crate_version, App, Arg, ArgMatches};
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use tracing::{error, info, warn};
use tracing_log::LogTracer;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::Registry;

#[async_std::main]
async fn main() {
    let matches = App::new("almetica")
        .version(crate_version!())
        .author("Almetica <almetica@protonmail.com>")
        .about("Custom server implementation for the game TERA")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .about("Sets a custom config file")
                .default_value("config.yaml")
                .takes_value(true),
        )
        .arg(
            Arg::new("log")
                .short('l')
                .long("log")
                .value_name("LEVEL")
                .about("Sets the log level")
                .default_value("INFO")
                .possible_values(&["ERROR", "WARN", "INFO", "DEBUG", "TRACE"])
                .takes_value(true),
        )
        .subcommand(App::new("run").about("Starts the game server"))
        .subcommand(
            App::new("create-account")
                .about("Creates an account")
                .arg(
                    Arg::new("name")
                        .short('n')
                        .long("name")
                        .about("name of the account")
                        .required(true)
                        .takes_value(true),
                )
                .arg(
                    Arg::new("password")
                        .short('p')
                        .long("password")
                        .about("password of the account")
                        .required(true)
                        .takes_value(true),
                ),
        )
        .get_matches();

    init_logging(&matches);

    if let Err(e) = run_command(&matches).await {
        error!("Error while executing program: {:?}", e);
        process::exit(1);
    }
}

fn init_logging(matches: &ArgMatches) {
    let level = match matches.value_of("log").unwrap_or_default() {
        "ERROR" => LevelFilter::ERROR,
        "WARN" => LevelFilter::WARN,
        "INFO" => LevelFilter::INFO,
        "DEBUG" => LevelFilter::DEBUG,
        "TRACE" => LevelFilter::TRACE,
        _ => LevelFilter::INFO,
    };

    let fmt_layer = Layer::default().with_target(true);
    let filter_layer = EnvFilter::from_default_env()
        .add_directive(level.into())
        .add_directive("async_h1=info".parse().unwrap())
        .add_directive("async_std=warn".parse().unwrap())
        .add_directive("mio=info".parse().unwrap())
        .add_directive("sqlx::query=info".parse().unwrap())
        .add_directive("tide=debug".parse().unwrap())
        .add_directive("tokio_postgres=info".parse().unwrap())
        .add_directive("tokio_util=info".parse().unwrap())
        .add_directive("tokio_postgres=info".parse().unwrap());

    let subscriber = Registry::default().with(filter_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    LogTracer::init().unwrap();
}

async fn run_command(matches: &ArgMatches) -> Result<()> {
    let config_str = matches.value_of("config").unwrap_or("config.yaml");
    let path = PathBuf::from(config_str);
    let config =
        read_configuration(&path).context(format!("Can't read configuration file {:?}", path))?;

    if let Some(matches) = matches.subcommand_matches("run") {
        info!("Starting almetica version {}", crate_version!());
        start_server(matches, &config).await?;
    } else if let Some(matches) = matches.subcommand_matches("create-account") {
        create_account(matches, &config).await?;
    }
    Ok(())
}

async fn start_server(_matches: &ArgMatches, config: &Configuration) -> Result<()> {
    info!("Reading opcode mapping file");
    let (opcode_mapping, reverse_opcode_mapping) = load_opcode_mapping(&config.data.path).context(
        format!("Can't read opcode mapping file {:?}", &config.data.path),
    )?;

    info!(
        "Loaded opcode mapping table with {} entries",
        opcode_mapping
            .iter()
            .filter(|&op| *op != Opcode::UNKNOWN)
            .count()
    );

    info!("Updating database schema");
    migrations::apply(
        format!(
            "postgres://{}:{}@{}:{}",
            config.database.username,
            config.database.password,
            config.database.hostname,
            config.database.port,
        )
        .as_ref(),
        &config.database.database,
    )
    .await
    .context("Can't update database schema")?;

    info!("Creating database pool");
    let pool = sqlx_pool(&config).await?;

    info!("Starting the ECS");
    let (global_world_handle, global_tx_channel) = start_global_world(config.clone(), pool.clone());

    info!("Starting the web server");
    let web_handle = start_web_server(pool, config.clone());

    info!("Starting the network server");
    let network_handle = start_network_server(
        global_tx_channel,
        opcode_mapping,
        reverse_opcode_mapping,
        config.clone(),
    );

    let (global_world_res, web_server_res, network_server_res) =
        join!(global_world_handle, web_handle, network_handle).await;

    global_world_res.context("Error while running the global world")?;
    web_server_res.context("Error while running the web server")?;
    network_server_res.context("Error while running the network server")?;

    Ok(())
}

/// Starts the global world on a new thread and returns a channel into the global world.
fn start_global_world(
    config: Configuration,
    pool: PgPool,
) -> (JoinHandle<Result<()>>, Sender<EcsMessage>) {
    let mut global_world = GlobalWorld::new(&config, &pool);
    let channel = global_world.channel.clone();
    let join_handle = task::spawn_blocking(move || {
        global_world.run();
        Ok(())
    });

    (join_handle, channel)
}

/// Starts the web server handling all HTTP requests.
fn start_web_server(pool: PgPool, config: Configuration) -> JoinHandle<Result<()>> {
    task::spawn(async {
        webserver::run(pool, config)
            .await
            .context("Can't run the web server")
    })
}

/// Starts the network server that handles all TCP game client connections.
fn start_network_server(
    global_channel: Sender<EcsMessage>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
    config: Configuration,
) -> JoinHandle<Result<()>> {
    task::spawn(async { networkserver::run(global_channel, map, reverse_map, config).await })
}

async fn sqlx_pool(config: &Configuration) -> Result<PgPool> {
    Ok(PgPool::new(
        format!(
            "postgres://{}:{}@{}:{}/{}",
            config.database.username,
            config.database.password,
            config.database.hostname,
            config.database.port,
            config.database.database
        )
        .as_ref(),
    )
    .await?)
}

async fn create_account(matches: &ArgMatches, config: &Configuration) -> Result<()> {
    let mut conn = sqlx_pool(&config).await?.acquire().await?;

    let account_name = matches.value_of("name").unwrap_or_default();
    let password = matches.value_of("password").unwrap_or_default();

    match account::get_by_name(&mut conn, account_name).await {
        Err(e) => match e.downcast_ref::<sqlx::Error>() {
            Some(sqlx::Error::RowNotFound) => {
                let hash =
                    password_hash::create_hash(password.as_bytes(), PasswordHashAlgorithm::Argon2)?;
                let acc = account::create(
                    &mut conn,
                    &Account {
                        id: -1,
                        name: account_name.to_string(),
                        password: hash,
                        algorithm: PasswordHashAlgorithm::Argon2,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    },
                )
                .await?;
                info!("Created account {} with ID {}", acc.name, acc.id);
            }
            Some(..) | None => {
                bail!(e);
            }
        },
        Ok(acc) => {
            error!("Account {} already exists with ID {}", acc.name, acc.id);
        }
    }
    Ok(())
}
