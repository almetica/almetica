#![warn(clippy::all)]
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use almetica::config::read_configuration;
use almetica::dataloader::load_opcode_mapping;
use almetica::ecs::event::Event;
use almetica::ecs::world::Multiverse;
use almetica::protocol::opcode::Opcode;
use almetica::protocol::GameSession;

use almetica::Result;
use clap::Clap;
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tokio::task;
use tracing::{error, info, info_span};
use tracing_futures::Instrument;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::Registry;

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

fn init_logging() {
    let fmt_layer = Layer::builder().with_target(true).finish();
    let filter_layer = EnvFilter::from_default_env().add_directive("legion_systems::system=warn".parse().unwrap());
    let subscriber = Registry::default().with(filter_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

async fn run() -> Result<()> {
    let opts: Opts = Opts::parse();

    info!("Reading configuration file");
    let config = match read_configuration(&opts.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Can't read configuration file {}: {:?}", &opts.config.display(), e);
            return Err(e);
        }
    };

    info!("Reading opcode mapping file");
    let opcode_mapping = match load_opcode_mapping(&config.data.path) {
        Ok(mapping) => {
            info!(
                "Loaded opcode mapping table with {} entries",
                mapping.iter().filter(|&op| *op != Opcode::UNKNOWN).count()
            );
            Arc::new(mapping)
        }
        Err(e) => {
            error!("Can't read opcode mapping file {}: {:?}", &opts.config.display(), e);
            return Err(e);
        }
    };

    let mut c: i64 = -1;
    let reverse_opcode_mapping = Arc::new(
        opcode_mapping
            .iter()
            .filter(|&op| *op != Opcode::UNKNOWN)
            .map(|op| {
                c += 1;
                (*op, c as u16)
            })
            .collect::<HashMap<Opcode, u16>>(),
    );

    info!("Starting the ECS multiverse");
    let global_tx_channel = start_multiverse();

    info!("Starting the network server on 127.0.0.1:10001");
    let mut listener = TcpListener::bind("127.0.0.1:10001").await?;

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                let thread_channel = global_tx_channel.clone();
                let thread_opcode_mapping = opcode_mapping.clone();
                let thread_reverse_opcode_mapping = reverse_opcode_mapping.clone();

                tokio::spawn(async move {
                    let span = info_span!("socket", %addr);
                    let _enter = span.enter();

                    info!("Incoming connection");
                    match GameSession::new(
                        &mut socket,
                        thread_channel,
                        thread_opcode_mapping,
                        thread_reverse_opcode_mapping,
                    )
                    .await
                    {
                        Ok(mut session) => {
                            let connection = session.connection;
                            match session
                                .handle_connection()
                                .instrument(info_span!("connection", connection = %connection))
                                .await
                            {
                                Ok(_) => info!("Closed connection"),
                                Err(e) => error!("Error while handling game session: {:?}", e),
                            }
                        }
                        Err(e) => error!("Failed create game session: {:?}", e),
                    }
                });
            }
            Err(e) => error!("Failed to open connection: {:?}", e),
        }
    }
}

// Starts the multiverse on a new thread and returns a channel into the global world.
fn start_multiverse() -> Sender<Arc<Event>> {
    let mut multiverse = Multiverse::new();
    let rx = multiverse.get_global_input_event_channel();

    task::spawn_blocking(move || {
        multiverse.run();
    });

    rx
}
