use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use std::process;

use anyhow::{anyhow, Context, Result};
use almetica::crypt::CryptSession;
use almetica::config::load_configuration;
use almetica::dataloader;
use almetica::protocol::opcode::Opcode;
use byteorder::{ByteOrder, LittleEndian};
use clap::Clap;
use log::{info, error};

const BUFFER_SIZE: usize = 2048;

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,

    #[clap(name = "FILE", parse(from_os_str))]
    files: Vec<PathBuf>,
}

/// Parses the given tcp stream dump.
/// File should be binary and contain the data as an array of items specified as:
///    u64 timestamp in ns
///    u64 length of packet data
///    PACKET DATA BYTES
///
fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let config = load_configuration(&opts.config)
        .with_context(|| format!("Can't load configuration file with path: {}", opts.config.display()))?;
    let opcode_mapping = load_opcode_mapping(&config.data.path)
        .with_context(|| format!("Can't load opcode mapping file with path: {}", config.data.path.display()))?;

    info!("Loaded opcode mapping table with {} entries.", opcode_mapping.len());

    let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut client_key_1: [u8; 128] = [0; 128];
    let mut client_key_2: [u8; 128] = [0; 128];
    let mut server_key_1: [u8; 128] = [0; 128];
    let mut server_key_2: [u8; 128] = [0; 128];
    
    info!("Start parsing stream.");
    for path in opts.files {
        let mut f = File::open(path.clone())?;
        f.read_exact(&mut buffer[..2])?;
    
        // TODO this can't work since we need to first read the packet
        let magic_word = LittleEndian::read_u16(&buffer);
        if magic_word != 1 {
            error!("Missing magic byte in stream of file: {}", path.display());
            return Err(anyhow!("missing magic byte"));
        }
    
        f.read_exact(&mut client_key_1)?;
        f.read_exact(&mut server_key_1)?;
        f.read_exact(&mut client_key_2)?;
        f.read_exact(&mut server_key_2)?;
    
        let session = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);

        // TODO: Read the packets
    }
    info!("Finished parsing stream.");
    Ok(())
}

fn load_opcode_mapping(data_path: &PathBuf) -> Result<Vec<Opcode>> {
    let mut path = data_path.clone();
    path.push("opcode.yaml");
    let file = File::open(path)?;
    let mut buffered = BufReader::new(file);
    let opcodes = dataloader::read_opcode_table(&mut buffered)?;
    Ok(opcodes)
}

fn main() {
    pretty_env_logger::init();
    if let Err(e) = run() {
        error!("Error while executing program: {}", e);
        process::exit(1);
    }   
}
