use std::io::BufReader;
use std::path::PathBuf;
use std::process;

use almetica::crypt::CryptSession;
use almetica::config::load_configuration;
use almetica::dataloader;
use almetica::Error;
use almetica::protocol::opcode::Opcode;
use byteorder::{ByteOrder, LittleEndian};
use clap::Clap;
use log::{info, error};
use tokio::fs::File;
use tokio::prelude::*;

pub type Result<T, E = Error> = std::result::Result<T, E>;

const BUFFER_SIZE: usize = 2048;

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,

    #[clap(name = "FILE", parse(from_os_str))]
    files: Vec<PathBuf>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    if let Err(e) = run().await {
        error!("Error while executing program: {}", e);
        process::exit(1);
    }
}


/// Parses the given tcp stream dump.
/// File should be binary and contain the data as an array of items specified as:
///    u64 timestamp in ns
///    u64 length of packet data
///    PACKET DATA BYTES
///
async fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let config = match load_configuration(&opts.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Can't read configuration file {}: {}", &opts.config.display(), e);
            return Err(e);
        }
    };
    
    let opcode_mapping = match load_opcode_mapping(&config.data.path) {
        Ok(o) => {
            info!("Loaded opcode mapping table with {} entries.", o.len());
            o
        }
        Err(e) => {
            error!("Can't read opcode mapping file {}: {}", &opts.config.display(), e);
            return Err(e);
        }
    };

    let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut client_key_1: [u8; 128] = [0; 128];
    let mut client_key_2: [u8; 128] = [0; 128];
    let mut server_key_1: [u8; 128] = [0; 128];
    let mut server_key_2: [u8; 128] = [0; 128];
    
    info!("Start parsing stream.");
    for path in opts.files {
        let mut f = File::open(path.clone()).await?;
        f.read_exact(&mut buffer[..2]).await?;
    
        // TODO this can't work since we need to first read the packet
        let magic_word = LittleEndian::read_u16(&buffer);
        if magic_word != 1 {
            error!("Missing magic byte in stream of file: {}", path.display());
            return Err(Error::NoMagicWord);
        }
    
        f.read_exact(&mut client_key_1).await?;
        f.read_exact(&mut server_key_1).await?;
        f.read_exact(&mut client_key_2).await?;
        f.read_exact(&mut server_key_2).await?;
    
        let session = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);

        // TODO: Read the packets
    }
    info!("Finished parsing stream.");
    Ok(())
}

fn load_opcode_mapping(data_path: &PathBuf) -> Result<Vec<Opcode>> {
    let mut path = data_path.clone();
    path.push("opcode.yaml");
    let file = std::fs::File::open(path)?;
    let mut buffered = BufReader::new(file);
    let opcodes = dataloader::read_opcode_table(&mut buffered)?;
    Ok(opcodes)
}

fn read_packet() {

}

fn parse_packet() {
    
}
