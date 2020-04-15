#![warn(clippy::all)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use bb8;
use bb8_postgres;
use clap::Clap;
use postgres::{self, NoTls};
use r2d2;
use r2d2_postgres;
use tokio::sync::mpsc::Sender;
use tokio::task::{self, JoinHandle};
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
use almetica::{AsyncDbPool, Error, Result};

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,
}

#[tokio::main]
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

    // We have two pools, one for the webserver and one for the gameserver. A DDOS should not be able
    // to steal all the database connections. Sync postgres does spawn it's own tokio runtime though
    // so it's not the most efficient process right now. This has definitely room for improvement.
    info!("Create async database pool");
    let asnyc_db_conf = assemble_async_db_config(&config);
    let asnyc_manager = bb8_postgres::PostgresConnectionManager::new(asnyc_db_conf.clone(), NoTls);
    let async_pool = bb8::Pool::builder()
        .max_size(2)
        .build(asnyc_manager)
        .await?;

    info!("Run database migrations");
    let (mut client, _) = asnyc_db_conf.connect(NoTls).await?;
    migrations::runner().run_async(&mut client).await?;

    info!("Starting the ECS multiverse");
    let (multiverse_handle, global_tx_channel) = start_multiverse(config.clone());

    info!("Starting the web server");
    let web_handle = start_web_server(async_pool, config.clone());

    info!("Starting the network server");
    let network_handle = start_network_server(
        global_tx_channel,
        opcode_mapping,
        reverse_opcode_mapping,
        config,
    );

    if let Err(e) = tokio::try_join!(multiverse_handle, web_handle, network_handle) {
        return Err(Error::TokioJoinError(e));
    }

    Ok(())
}

fn init_logging() {
    let fmt_layer = Layer::default().with_target(false);
    let filter_layer =
        EnvFilter::from_default_env().add_directive("legion_systems::system=warn".parse().unwrap());
    let subscriber = Registry::default().with(filter_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    LogTracer::init().unwrap();
}

/// Starts the multiverse on a new thread and returns a channel into the global world.
fn start_multiverse(config: Configuration) -> (JoinHandle<()>, Sender<Arc<Event>>) {
    let mut multiverse = Multiverse::new();
    let rx = multiverse.get_global_input_event_channel();

    let join_handle = task::spawn_blocking(move || {
        info!("Create sync database pool");
        let manager =
            r2d2_postgres::PostgresConnectionManager::new(assemble_sync_db_config(&config), NoTls);
        let pool = r2d2::Pool::builder().max_size(20).build(manager).unwrap();

        multiverse.run(pool, config);
    });

    (join_handle, rx)
}

/// Starts the web server handling all HTTP requests.
fn start_web_server(pool: AsyncDbPool, config: Configuration) -> JoinHandle<()> {
    task::spawn(async {
        webserver::run(pool, config).await;
    })
}

/// Starts the network server.
fn start_network_server(
    global_channel: Sender<Arc<Event>>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
    config: Configuration,
) -> JoinHandle<Result<()>> {
    task::spawn(async { networkserver::run(global_channel, map, reverse_map, config).await })
}

fn assemble_sync_db_config(config: &Configuration) -> postgres::Config {
    let mut c = postgres::Config::new();
    c.host(&config.database.hostname);
    c.port(config.database.port);
    c.user(&config.database.username);
    c.password(&config.database.password);
    c.dbname(&config.database.database);
    c
}

fn assemble_async_db_config(config: &Configuration) -> tokio_postgres::Config {
    let mut c = tokio_postgres::Config::new();
    c.host(&config.database.hostname);
    c.port(config.database.port);
    c.user(&config.database.username);
    c.password(&config.database.password);
    c.dbname(&config.database.database);
    c
}
