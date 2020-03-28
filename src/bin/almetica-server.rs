use std::path::PathBuf;
use std::process;

use almetica::Result;
use almetica::config::load_configuration;
use almetica::dataloader::load_opcode_mapping;
use almetica::protocol::GameSession;
use clap::Clap;
use log::{error, info};
use tokio::net::TcpListener;

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
    let _opcode_mapping = match load_opcode_mapping(&config.data.path) {
        Ok(o) => {
            info!("Loaded opcode mapping table with {} entries.", o.len());
            o
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

    info!("Starting server on 127.0.0.1:1001");
    let mut listener = TcpListener::bind("127.0.0.1:1001").await?;

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                info!("Incoming connection from client {:?}", addr);
                match GameSession::new(&mut socket, addr).await {
                    Ok(session) => {
                        match session.handle_connection().await {
                            Ok(_) => info!("Closed connection from client {:?}", addr),
                            Err(e) => error!("Error while handling game session for client {:?}: {:?}", addr, e),
                        }
                    },
                    Err(e) => error!("Failed create game session with client {:?}: {:?}", addr, e),
                }
            },
            Err(e) => error!("Failed to open connection with client: {:?}", e),
        }
    }
}
