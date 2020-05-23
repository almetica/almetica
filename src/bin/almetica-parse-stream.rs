#![warn(clippy::all)]
use almetica::config::read_configuration;
use almetica::crypt::CryptSession;
use almetica::dataloader::load_opcode_mapping;
use almetica::protocol::opcode::Opcode;
use almetica::Result;
use anyhow::{bail, ensure, Context};
use byteorder::{ByteOrder, LittleEndian};
use clap::Clap;
use hex::encode;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process;
use tracing::{debug, error, info, warn};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::Registry;

#[derive(Clap)]
#[clap(version = "0.0.1", author = "Almetica <almetica@protonmail.com>")]
struct Opts {
    #[clap(short = "c", long = "config", default_value = "config.yaml")]
    config: PathBuf,

    #[clap(name = "FILE", parse(from_os_str))]
    files: Vec<PathBuf>,
}

fn main() {
    init_logging();

    if let Err(e) = run() {
        error!("Error while executing program: {}", e);
        process::exit(1);
    }
}

fn init_logging() {
    let fmt_layer = Layer::default().with_target(false);
    let filter_layer = EnvFilter::from_default_env();
    let subscriber = Registry::default().with(filter_layer).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

/// Parses the given tcp stream dump.
/// File should be binary and contain the data as an array of items specified as:
///    i8  server (1) or client (0)
///    i64 length of packet data
///    PACKET DATA BYTES
///
fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let config = read_configuration(&opts.config).context(format!(
        "Can't read configuration file {}",
        &opts.config.display(),
    ))?;
    let (opcode_mapping, _reverse_opcode_mapping) = load_opcode_mapping(&config.data.path)
        .context(format!(
            "Can't read opcode mapping file {}",
            &opts.config.display(),
        ))?;

    info!(
        "Loaded opcode mapping table with {} entries",
        opcode_mapping
            .iter()
            .filter(|&op| *op != Opcode::UNKNOWN)
            .count()
    );

    info!("Start parsing stream.");
    for path in opts.files {
        let mut file = File::open(path.clone())?;

        let mut buffer: [u8; 9] = [0; 9];
        let mut sp = StreamParser {
            state: -1,
            num_unknown: 0,
            num_packets: 0,
            crypt_session: None,
            opcode: opcode_mapping.clone(),
            client_key_1: vec![0; 128],
            client_key_2: vec![0; 128],
            server_key_1: vec![0; 128],
            server_key_2: vec![0; 128],
            tmp_buffer: [Vec::with_capacity(4096), Vec::with_capacity(4096)],
        };

        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                info!("Reached end of stream.");
                break;
            }
            let is_server = buffer[0] as usize;
            let length = LittleEndian::read_i64(&buffer[1..]) as usize;

            let mut payload_buffer = vec![0; length];
            file.read_exact(&mut payload_buffer[..length as usize])?;
            sp.parse_packet(is_server, &mut payload_buffer)?;
        }
        if sp.num_unknown > 0 {
            warn!(
                "Found {} of {} packets with unknown type!",
                sp.num_unknown, sp.num_packets
            );
        }
    }
    info!("Finished parsing files.");
    Ok(())
}

/// Struct to parse the provided stream. Only useful for this parser.
struct StreamParser {
    state: i8,
    num_unknown: usize,
    num_packets: usize,
    crypt_session: Option<CryptSession>,
    opcode: Vec<Opcode>,
    client_key_1: Vec<u8>,
    client_key_2: Vec<u8>,
    server_key_1: Vec<u8>,
    server_key_2: Vec<u8>,
    tmp_buffer: [Vec<u8>; 2],
}

impl StreamParser {
    /// Parses a packet in the payload. Handles the crypt session initialization.
    pub fn parse_packet(&mut self, is_server: usize, payload: &mut Vec<u8>) -> Result<()> {
        if self.state != 4 {
            self.init_crypt_session(is_server, payload)?;
            return Ok(());
        }

        match &mut self.crypt_session {
            Some(c) => {
                if is_server == 1 {
                    c.crypt_server_data(payload);
                } else {
                    c.crypt_client_data(payload);
                }
            }
            None => {
                bail!("Crypt session not initialized");
            }
        }

        self.tmp_buffer[is_server].append(payload);
        loop {
            if self.tmp_buffer[is_server].len() < 4 {
                return Ok(());
            }
            let length = LittleEndian::read_u16(&self.tmp_buffer[is_server][0..2]) as usize;
            let opcode = LittleEndian::read_u16(&self.tmp_buffer[is_server][2..4]);
            if length <= self.tmp_buffer[is_server].len() {
                let packet_type = &self.opcode[opcode as usize];
                if *packet_type == Opcode::UNKNOWN {
                    self.num_unknown += 1;
                }

                let mut packet_data = vec![0; length - 4];
                packet_data.copy_from_slice(&self.tmp_buffer[is_server][4..length]);
                self.tmp_buffer[is_server].copy_within(length.., 0);
                self.tmp_buffer[is_server].resize(self.tmp_buffer[is_server].len() - length, 0);

                if is_server == 1 {
                    info!(
                        "Found packet {:?} from server. Length: {} Rest: {}",
                        packet_type,
                        packet_data.len(),
                        self.tmp_buffer[1].len()
                    );
                } else {
                    info!(
                        "Found packet {:?} from client. Length: {} Rest: {}",
                        packet_type,
                        packet_data.len(),
                        self.tmp_buffer[0].len()
                    );
                }
                self.pretty_data_print(packet_data);
                self.num_packets += 1;
            } else {
                return Ok(());
            }
        }
    }

    fn pretty_data_print(&self, data: Vec<u8>) {
        let mut s = String::default();
        let mut first = true;
        for b in data {
            if first {
                first = false;
            } else {
                s += ", ";
            }

            s += format!("0x{:x}", b).as_ref();
        }
        debug!("[{}]", s);
    }

    fn init_crypt_session(&mut self, is_server: usize, mut payload: &[u8]) -> Result<()> {
        match self.state {
            -1 => {
                ensure!(is_server == 1, "Unexpected packet from client");
                let magic_word = LittleEndian::read_u32(&payload[..4]);
                if magic_word != 1 {
                    bail!("No magic word found in stream");
                }
                self.state = 0;
            }
            0 => {
                ensure!(is_server == 0, "Unexpected packet from server");
                payload.read_exact(&mut self.client_key_1)?;
                self.state = 1;
            }
            1 => {
                ensure!(is_server == 1, "Unexpected packet from client");
                payload.read_exact(&mut self.server_key_1)?;
                self.state = 2;
            }
            2 => {
                ensure!(is_server == 0, "Unexpected packet from server");
                payload.read_exact(&mut self.client_key_2)?;
                self.state = 3;
            }
            3 => {
                ensure!(is_server == 1, "Unexpected packet from client");
                payload.read_exact(&mut self.server_key_2)?;

                debug!("ClientKey1 {}", encode(&self.client_key_1));
                debug!("ClientKey2 {}", encode(&self.client_key_2));
                debug!("ServerKey1 {}", encode(&self.server_key_1));
                debug!("ServerKey2 {}", encode(&self.server_key_2));

                self.crypt_session = Some(CryptSession::new(
                    [self.client_key_1.clone(), self.client_key_2.clone()],
                    [self.server_key_1.clone(), self.server_key_2.clone()],
                ));
                self.state = 4;
                info!("Crypt session initialized.");
            }
            _ => bail!("Unexpected crypt init sequence"),
        }
        Ok(())
    }
}
