use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process;

use almetica::config::load_configuration;
use almetica::crypt::CryptSession;
use almetica::dataloader;
use almetica::protocol::opcode::Opcode;
use almetica::Error;
use byteorder::{ByteOrder, LittleEndian};
use clap::Clap;
use hex::encode;
use log::{debug, error, info, warn};
use tokio::fs::File;

pub type Result<T, E = Error> = std::result::Result<T, E>;

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
///    i8  server (1) or client (0)
///    i64 length of packet data
///    PACKET DATA BYTES
///
async fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let config = match load_configuration(&opts.config) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Can't read configuration file {}: {}",
                &opts.config.display(),
                e
            );
            return Err(e);
        }
    };
    let opcode_mapping = match load_opcode_mapping(&config.data.path) {
        Ok(o) => {
            info!("Loaded opcode mapping table with {} entries.", o.len());
            o
        }
        Err(e) => {
            error!(
                "Can't read opcode mapping file {}: {}",
                &opts.config.display(),
                e
            );
            return Err(e);
        }
    };
    info!("Start parsing stream.");
    for path in opts.files {
        let mut file = File::open(path.clone()).await?;

        let mut buffer: [u8; 9] = [0; 9];
        let mut sp = StreamParser {
            state: -1,
            crypt_session: None,
            opcode: opcode_mapping.clone(),
            client_key_1: vec![0; 128],
            client_key_2: vec![0; 128],
            server_key_1: vec![0; 128],
            server_key_2: vec![0; 128],
            num_unknown: 0,
            num_packets: 0,
        };

        loop {
            let read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer).await?;
            if read == 0 {
                info!("Reached end of stream.");
                break;
            }
            let is_server = buffer[0];
            let length = LittleEndian::read_i64(&buffer[1..]) as usize;

            let mut payload_buffer = vec![0; length];
            tokio::io::AsyncReadExt::read_exact(&mut file, &mut payload_buffer[..length as usize]).await?;
            sp.parse_packet(is_server, &mut payload_buffer).await?;
        }
        
        if sp.num_unknown > 0 {
            warn!("Found {} of {} packets with unknown type!", sp.num_unknown, sp.num_packets);
        }
    }
    info!("Finished parsing files.");
    Ok(())
}

pub fn load_opcode_mapping(data_path: &PathBuf) -> Result<Vec<Opcode>> {
    let mut path = data_path.clone();
    path.push("opcode.yaml");
    let file = std::fs::File::open(path)?;
    let mut buffered = BufReader::new(file);
    let opcodes = dataloader::read_opcode_table(&mut buffered)?;
    Ok(opcodes)
}

/// Struct to parse the provided stream. Only usefull for this parser.
struct StreamParser {
    state: i8,
    crypt_session: Option<CryptSession>,
    opcode: Vec<Opcode>,
    client_key_1: Vec<u8>,
    client_key_2: Vec<u8>,
    server_key_1: Vec<u8>,
    server_key_2: Vec<u8>,
    num_unknown: usize,
    num_packets: usize,
}

impl StreamParser {
    /// Parses a packet in the payload. Handles the crypt session initialization.
    pub async fn parse_packet(&mut self, is_server: u8, payload: &mut [u8]) -> Result<()> {
        if self.state != 4 {
            self.init_crypt_session(is_server, payload)?;
            return Ok(());
        }

        if is_server == 1 {
            match &mut self.crypt_session {
                Some(c) => c.crypt_server_data(payload),
                None => {
                    error!("Crypt session not initialized.");
                    return Err(Error::Unknown);
                },
            }
        } else {
            match &mut self.crypt_session {
                Some(c) => c.crypt_client_data(payload),
                None => {
                    error!("Crypt session not initialized.");
                    return Err(Error::Unknown);
                },
            }
        }

        // TODO there could be multiple game packets in one tcp message!
        let length = LittleEndian::read_u16(&payload[0..2]);
        let opcode = LittleEndian::read_u16(&payload[2..4]);

        let packet_type = &self.opcode[opcode as usize];
        if *packet_type == Opcode::UNKNOWN {
            self.num_unknown += 1;
        }

        if is_server == 1 {
            info!("Found packet {} from server. Length: {} Payload size: {}", packet_type, length, payload.len());
        } else {
            info!("Found packet {} from client. Length: {} Payload size: {}", packet_type, length, payload.len());
        }

        self.num_packets += 1;
        Ok(())
    }

    fn init_crypt_session(&mut self, is_server: u8, mut payload: &[u8]) -> Result<()> {
        match self.state {
            -1 => {
                if is_server != 1 {
                    error!("Unexpected message from client!");
                    return Err(Error::Unknown);
                }
                let magic_word = LittleEndian::read_u32(&payload[..4]);
                if magic_word != 1 {
                    error!("Missing magic byte in stream of file!");
                    return Err(Error::NoMagicWord);
                }
                self.state = 0;
            }
            0 => {
                if is_server != 0 {
                    error!("Unexpected packet from server!");
                    return Err(Error::Unknown);
                }
                payload.read_exact(&mut self.client_key_1)?;
                self.state = 1;
            }
            1 => {
                if is_server != 1 {
                    error!("Unexpected packet from client!");
                    return Err(Error::Unknown);
                }
                payload.read_exact(&mut self.server_key_1)?;
                self.state = 2;
            }
            2 => {
                if is_server != 0 {
                    error!("Unexpected packet from server!");
                    return Err(Error::Unknown);
                }
                payload.read_exact(&mut self.client_key_2)?;
                self.state = 3;
            }
            3 => {
                if is_server != 1 {
                    error!("Unexpected packet from client!");
                    return Err(Error::Unknown);
                }
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
            _ => return Err(Error::Unknown),
        }
        Ok(())
    }
}
