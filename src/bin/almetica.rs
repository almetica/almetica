#![warn(clippy::all)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use clap::Clap;
use mysql::Pool;
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
use almetica::gameserver;
use almetica::protocol::opcode::Opcode;
use almetica::webserver;
use almetica::{Error, Result};

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

    info!("Create database pool");
    let pool = Pool::new(assemble_db_string(&config))?;

    info!("Starting the ECS multiverse");
    let (multiverse_handle, global_tx_channel) = start_multiverse(pool.clone(), config.clone());

    info!("Starting the web server");
    let web_handle = start_web_server(pool, config.clone());

    info!("Starting the game server");
    let game_handle = start_game_server(
        global_tx_channel,
        opcode_mapping,
        reverse_opcode_mapping,
        config,
    );

    if let Err(e) = tokio::try_join!(multiverse_handle, web_handle, game_handle) {
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
fn start_multiverse(pool: Pool, config: Configuration) -> (JoinHandle<()>, Sender<Arc<Event>>) {
    let mut multiverse = Multiverse::new();
    let rx = multiverse.get_global_input_event_channel();

    let join_handle = task::spawn_blocking(move || {
        multiverse.run(pool, config);
    });

    (join_handle, rx)
}

/// Starts the web server handling all HTTP requests.
fn start_web_server(pool: Pool, config: Configuration) -> JoinHandle<()> {
    task::spawn(async {
        webserver::run(pool, config).await;
    })
}

/// Starts the game server.
fn start_game_server(
    global_channel: Sender<Arc<Event>>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
    config: Configuration,
) -> JoinHandle<Result<()>> {
    task::spawn(async { gameserver::run(global_channel, map, reverse_map, config).await })
}

fn assemble_db_string(config: &Configuration) -> String {
    format!(
        "mysql://{}:{}@{}:{}/{}",
        config.database.username,
        config.database.password,
        config.database.hostname,
        config.database.port,
        config.database.database
    )
}
