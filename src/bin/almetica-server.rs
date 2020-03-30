use std::path::PathBuf;
use std::process;

use almetica::config::load_configuration;
use almetica::dataloader::load_opcode_mapping;
use almetica::ecs::event::Event;
use almetica::ecs::world::Multiverse;
use almetica::protocol::opcode::Opcode;
use almetica::protocol::GameSession;

use almetica::Result;
use clap::Clap;
use log::{error, info};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tokio::task;

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    if let Err(e) = run().await {
        error!("Error while executing program: {:?}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let opts: Opts = Opts::parse();

    info!("Loading configuration file");
    let config = match load_configuration(&opts.config) {
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
    let opcode_mapping = match load_opcode_mapping(&config.data.path) {
        Ok(mapping) => {
            info!(
                "Loaded opcode mapping table with {} entries",
                mapping.iter().filter(|&op| *op != Opcode::UNKNOWN).count()
            );
            mapping
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

    info!("Starting the ECS multiverse");
    let global_tx_channel = start_multiverse();

    info!("Starting the network server on 0.0.0.0:10001");
    let mut listener = TcpListener::bind("0.0.0.0:10001").await?;

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                info!("Incoming connection on socket {:?}", addr);
                match GameSession::new(
                    &mut socket,
                    addr,
                    global_tx_channel.clone(),
                    &opcode_mapping,
                )
                .await
                {
                    Ok(mut session) => match session.handle_connection().await {
                        Ok(_) => info!("Closed connection on socket {:?}", addr),
                        Err(e) => error!(
                            "Error while handling game session on socket {:?}: {:?}",
                            addr, e
                        ),
                    },
                    Err(e) => error!("Failed create game session on socket {:?}: {:?}", addr, e),
                }
            }
            Err(e) => error!("Failed to open connection on socket: {:?}", e),
        }
    }
}

// Starts the multiverse on a new thread and returns a channel into the global world.
fn start_multiverse() -> Sender<Box<Event>> {
    let mut multiverse = Multiverse::new();
    let rx = multiverse.global_world_handle.tx_channel.clone();

    task::spawn_blocking(move || {
        multiverse.run();
    });

    rx
}
