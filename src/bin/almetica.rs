#![warn(clippy::all)]
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use async_std::prelude::*;
use async_std::sync::Sender;
use async_std::task::{self, JoinHandle};
use clap::Clap;
use sqlx::PgPool;
use tokio::runtime::Runtime;
use tracing::{error, info, warn};
use tracing_log::LogTracer;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::Registry;

use almetica::config::{read_configuration, Configuration};
use almetica::dataloader::load_opcode_mapping;
use almetica::ecs::event::Event;
use almetica::ecs::world::Multiverse;
use almetica::model::embedded::migrations;
use almetica::networkserver;
use almetica::protocol::opcode::Opcode;
use almetica::webserver;
use almetica::Result;

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,
}

#[async_std::main]
async fn main() {
    init_logging();

    if let Err(e) = run().await {
        error!("Error while executing program: {:?}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let opts: Opts = Opts::parse();

    info!("Reading configuration file");
    let config = match read_configuration(&opts.config) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Can't read configuration file {}: {:?}",
                &opts.config.display(),
                e
            );
            return Err(e);
        }
    };

    info!("Reading opcode mapping file");
    let (opcode_mapping, reverse_opcode_mapping) = match load_opcode_mapping(&config.data.path) {
        Ok((opcode_mapping, reverse_opcode_mapping)) => {
            info!(
                "Loaded opcode mapping table with {} entries",
                opcode_mapping
                    .iter()
                    .filter(|&op| *op != Opcode::UNKNOWN)
                    .count()
            );
            (opcode_mapping, reverse_opcode_mapping)
        }
        Err(e) => {
            error!(
                "Can't read opcode mapping file {}: {:?}",
                &opts.config.display(),
                e
            );
            return Err(e);
        }
    };

    info!("Running database migrations");
    run_db_migrations(&config)?;

    info!("Creating database pool");
    let pool = PgPool::new(sqlx_config(&config).as_ref()).await?;

    info!("Starting the ECS multiverse");
    let (multiverse_handle, global_tx_channel) = start_multiverse(config.clone(), pool.clone());

    info!("Starting the web server");
    let web_handle = start_web_server(pool, config.clone());

    info!("Starting the network server");
    let network_handle = start_network_server(
        global_tx_channel,
        opcode_mapping,
        reverse_opcode_mapping,
        config,
    );

    let (_, err) = multiverse_handle
        .join(web_handle)
        .join(network_handle)
        .await;
    if let Err(e) = err {
        error!("Can't shutdown server gracefully: {:?}", e);
    }

    Ok(())
}

fn init_logging() {
    let fmt_layer = Layer::default().with_target(true);
    let filter_layer = EnvFilter::from_default_env();
    let subscriber = Registry::default().with(filter_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    LogTracer::init().unwrap();
}

/// Performs the database migrations
fn run_db_migrations(config: &Configuration) -> Result<()> {
    // FIXME: Use sqlx once refinery adds support for it or we implement our own migration framework.
    let mut rt = Runtime::new()?;
    rt.block_on(async {
        let db_conf = tokio_postgres_config(&config);
        let (mut client, connection) = db_conf.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("connection error: {}", e);
            }
        });
        migrations::runner().run_async(&mut client).await.unwrap();
    });
    Ok(())
}

/// Starts the multiverse on a new thread and returns a channel into the global world.
fn start_multiverse(config: Configuration, pool: PgPool) -> (JoinHandle<()>, Sender<Arc<Event>>) {
    let mut multiverse = Multiverse::new();
    let rx = multiverse.get_global_input_event_channel();

    let join_handle = task::spawn_blocking(move || {
        multiverse.run(pool, config);
    });

    (join_handle, rx)
}

/// Starts the web server handling all HTTP requests.
fn start_web_server(pool: PgPool, config: Configuration) -> JoinHandle<()> {
    task::spawn(async {
        if let Err(e) = webserver::run(pool, config).await {
            error!("Can't run the web server: {:?}", e);
        };
    })
}

/// Starts the network server that handles all TCP game client connections.
fn start_network_server(
    global_channel: Sender<Arc<Event>>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
    config: Configuration,
) -> JoinHandle<Result<()>> {
    task::spawn(async { networkserver::run(global_channel, map, reverse_map, config).await })
}

fn tokio_postgres_config(config: &Configuration) -> tokio_postgres::Config {
    let mut c = tokio_postgres::Config::new();
    c.host(&config.database.hostname);
    c.port(config.database.port);
    c.user(&config.database.username);
    c.password(&config.database.password);
    c.dbname(&config.database.database);
    c
}

fn sqlx_config(config: &Configuration) -> String {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        config.database.username,
        config.database.password,
        config.database.hostname,
        config.database.port,
        config.database.database
    )
}
