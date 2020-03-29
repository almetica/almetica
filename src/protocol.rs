/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::net::SocketAddr;

use super::*;
use super::crypt::CryptSession;
use opcode::Opcode;

use byteorder::{ByteOrder, LittleEndian};
use log::{debug, error, info, warn};
use rand::rngs::OsRng;
use rand_core::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Abstracts the game network protocol session.
pub struct GameSession<'a> {
    uid: Option<u64>, // User ID
    stream: &'a mut TcpStream,
    addr: SocketAddr,
    crypt: CryptSession,
    opcode_table: &'a [Opcode],
    // TODO Will later have TX/RX channels to the event handler
    // We should have two RX and two TX channels: One for the world and one for the instance ECS.
}

impl<'a> GameSession<'a> {
    /// Initializes and returns a `GameSession` object.
    pub async fn new(stream: &'a mut TcpStream, addr: SocketAddr, opcode_table: &'a [Opcode]) -> Result<GameSession<'a>> {
        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word on socket: {:?}", addr);
        match stream.write_all(&magic_word_buffer).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't send magic word on socket {:?}: {:?}", addr, e);
                return Err(Error::Io(e));
            }
        };

        match stream.read_exact(&mut client_key_1).await {
            Ok(_i) => (),
            Err(e) => {
                error!("Can't read client key 1 on socket {:?}: {:?}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Received client key 1 on socket {:?}", addr);

        OsRng.fill_bytes(&mut server_key_1);
        match stream.write_all(&server_key_1).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 1 on socket {:?}: {:?}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 1 on socket {:?}", addr);

        match stream.read_exact(&mut client_key_2).await {
            Ok(_i) => (),
            Err(e) => {
                error!("Can't read client key 2 on socket {:?}: {:?}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Received client key 2 on socket {:?}", addr);

        OsRng.fill_bytes(&mut server_key_2);
        match stream.write_all(&server_key_2).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 2 on socket {:?}: {:?}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 2 on socket {:?}", addr);

        let cs = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);
        let gs = GameSession {
            uid: None,
            stream: stream,
            addr: addr,
            crypt: cs,
            opcode_table: opcode_table,
        };

        info!("Game session initialized for socket: {:?}", addr);
        Ok(gs)
    }

    /// Handles the writing / sending on the TCP stream.
    pub async fn handle_connection(&mut self) -> Result<()> {      
        let mut header_buf = vec![0u8; 4];
        loop {
            // RX
            // TODO I assume that this return quite fast if no data is to read.
            let len = self.stream.peek(&mut header_buf).await?;
            if len >= 4 {
                self.stream.read_exact(&mut header_buf).await?;
                self.crypt.crypt_client_data(&mut header_buf);
                
                let packet_length = LittleEndian::read_u16(&header_buf[0..2]) as usize;
                let opcode = LittleEndian::read_u16(&header_buf[2..4]) as usize;

                let mut data_buf = vec![0u8; packet_length - 4];
                self.stream.read_exact(&mut data_buf).await?;
                self.crypt.crypt_client_data(&mut data_buf);

                self.decode_packet(opcode, data_buf);
            }
            // TX
            // TODO Query TX channel and send data
        }
    }

    /// Decodes a packet from the given `Vec<u8>`.
    fn decode_packet(&self, opcode: usize, _packet_data: Vec<u8>) {
        // TODO only forward handled packet data into the right ECS (either global or instance)
        let packet_type = &self.opcode_table[opcode];
        match packet_type {
            Opcode::C_CHECK_VERSION => {
                debug!("C_CHECK_VERSION received on socket {:?}", self.addr);
            }
            Opcode::C_LOGIN_ARBITER => {
                debug!("C_LOGIN_ARBITER received on socket {:?}", self.addr);
            },
            Opcode::UNKNOWN => {
                warn!("Unmapped and unhandled opcode on socket {:?}: {:?}", opcode, self.addr);
            }
            _ => {
                warn!("Mapped but unhandled opcode on socket {:?}: {:?}", opcode, self.addr);
            },
        }
    }
}

// TODO I think we can only write an integration test for the above code and we
// would need to actually open a TcpStream for this.
// Look at the integration tests that tokio have:
// https://github.com/tokio-rs/tokio/blob/master/tokio/tests/tcp_echo.rs

// Write test for connection

// Write test for RX

// Write test for TX
