/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::net::SocketAddr;

use super::crypt::CryptSession;
use super::ecs::event::Event;
use super::*;
use opcode::Opcode;

use byteorder::{ByteOrder, LittleEndian};
use log::{debug, error, info, warn};
use rand::rngs::OsRng;
use rand_core::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};

/// Abstracts the game network protocol session.
pub struct GameSession<'a> {
    stream: &'a mut TcpStream,
    addr: SocketAddr,
    crypt: CryptSession,
    opcode_table: &'a [Opcode],
    // Sending channel TO the global world
    global_tx_channel: Sender<Box<Event>>,
    // Receiving channel FROM the global world
    global_rx_channel: Option<Receiver<Box<Event>>>,
    // Sending channel TO the instance world
    instance_tx_channel: Option<Sender<Box<Event>>>,
    // Receiving channel FROM the instance world
    instance_rx_channel: Option<Receiver<Box<Event>>>,
}

impl<'a> GameSession<'a> {
    /// Initializes and returns a `GameSession` object.
    pub async fn new(
        stream: &'a mut TcpStream,
        addr: SocketAddr,
        global_tx_channel: Sender<Box<Event>>,
        opcode_table: &'a [Opcode],
    ) -> Result<GameSession<'a>> {
        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word on socket {:?}", addr);
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

        // TODO send Evvent::OpenConnection
        // TODO open channel and send TX to global world

        let cs = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);
        let gs = GameSession {
            stream: stream,
            addr: addr,
            crypt: cs,
            opcode_table: opcode_table,
            global_tx_channel: global_tx_channel,
            global_rx_channel: None,
            instance_tx_channel: None,
            instance_rx_channel: None,
        };

        info!("Game session initialized on socket {:?}", addr);
        Ok(gs)
    }

    /// Handles the writing / sending on the TCP stream.
    pub async fn handle_connection(&mut self) -> Result<()> {
        let mut header_buf = vec![0u8; 4];
        loop {
            tokio::select! {
                // RX
                result = self.stream.peek(&mut header_buf) => {
                    if let Ok(read_bytes) = result {
                        if read_bytes == 4 {
                            self.stream.read_exact(&mut header_buf).await?;
                            self.crypt.crypt_client_data(&mut header_buf);
                            let packet_length = LittleEndian::read_u16(&header_buf[0..2]) as usize;
                            let opcode = LittleEndian::read_u16(&header_buf[2..4]) as usize;
                            let mut data_buf = vec![0u8; packet_length - 4];
                            self.stream.read_exact(&mut data_buf).await?;
                            self.crypt.crypt_client_data(&mut data_buf);
                            self.handle_packet(opcode, data_buf).await?;
                        }
                    }
                }
                // TX
                // TODO Query TX channel and send data
            }
        }
    }

    /// Decodes a packet from the given `Vec<u8>`.
    async fn handle_packet(&mut self, opcode: usize, packet_data: Vec<u8>) -> Result<()> {
        let opcode_type = self.opcode_table[opcode];
        match opcode_type {
            Opcode::UNKNOWN => {
                warn!(
                    "Unmapped and unhandled packet {:?} on socket {:?}",
                    opcode, self.addr
                );
            }
            _ => {
                match Event::new_from_packet(opcode_type, packet_data) {
                    Ok(event) => {
                        debug!(
                            "Received valid packet {:?} on socket {:?}",
                            opcode_type, self.addr
                        );
                        self.global_tx_channel.send(Box::new(event)).await?;
                    }
                    Err(e) => match e {
                        Error::NoEventMappingForPacket => {
                            warn!(
                                "No mapping found for packet {:?} on socket {:?}",
                                opcode_type, self.addr
                            );
                        },
                        _ => error!(
                            "Can't create event from valid packet {:?} on socket {:?}: {:?}",
                            opcode_type, self.addr, e
                        ),
                    },
                }
            }
        }
        Ok(())
    }
}

// TODO I think we can only write an integration test for the above code and we
// would need to actually open a TcpStream for this.
// Look at the integration tests that tokio have:
// https://github.com/tokio-rs/tokio/blob/master/tokio/tests/tcp_echo.rs

// Write test for connection

// Write test for RX

// Write test for TX
