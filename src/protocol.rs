/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::crypt::CryptSession;
use crate::ecs::event::Event;
use crate::*;
use opcode::Opcode;

use byteorder::{ByteOrder, LittleEndian};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use rand_core::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};

/// Abstracts the game network protocol session.
pub struct GameSession<'a> {
    // User ID
    uid: u64,
    stream: &'a mut TcpStream,
    addr: SocketAddr,
    cipher: CryptSession,
    opcode_table: &'a [Opcode],
    reverse_opcode_table: &'a HashMap<Opcode, u16>,
    // Sending channel TO the global world
    global_request_channel: Sender<Arc<Event>>,
    // Receiving channel FROM the global world
    global_response_channel: Receiver<Arc<Event>>,
    // Sending channel TO the instance world
    _instance_request_channel: Option<Sender<Arc<Event>>>,
    // Receiving channel FROM the instance world
    _instance_response_channel: Option<Receiver<Arc<Event>>>,
}

impl<'a> GameSession<'a> {
    /// Initializes and returns a `GameSession` object.
    pub async fn new(
        stream: &'a mut TcpStream,
        addr: SocketAddr,
        mut global_request_channel: Sender<Arc<Event>>,
        opcode_table: &'a [Opcode],
        reverse_opcode_table: &'a HashMap<Opcode, u16>,
    ) -> Result<GameSession<'a>> {
        // Initialize the stream cipher with the client.
        let cipher = GameSession::init_crypto(stream, &addr).await?;

        // Channel to receive response events from the global world ECS.
        let (tx_response_channel, mut rx_response_channel) = channel(128);
        global_request_channel
            .send(Arc::new(Event::RequestRegisterConnection{
                uid: 0,
                response_channel: tx_response_channel,
            }))
            .await?;
        // Wait for the global ECS to return an UID for the connection.
        let message = rx_response_channel.recv().await;
        let uid = GameSession::parse_uid(message).await?;

        info!("Game session initialized on socket {:?}", addr);

        Ok(GameSession {
            uid,
            stream,
            addr,
            cipher,
            opcode_table,
            reverse_opcode_table,
            global_request_channel,
            global_response_channel: rx_response_channel,
            _instance_request_channel: None,
            _instance_response_channel: None,
        })
    }

    async fn init_crypto(stream: &mut TcpStream, addr: &SocketAddr) -> Result<CryptSession> {
        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word on socket {:?}", addr);
        if let Err(e) = stream.write_all(&magic_word_buffer).await {
            error!("Can't send magic word on socket {:?}: {:?}", addr, e);
            return Err(Error::Io(e));
        }

        if let Err(e) = stream.read_exact(&mut client_key_1).await {
            error!("Can't read client key 1 on socket {:?}: {:?}", addr, e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 1 on socket {:?}", addr);

        OsRng.fill_bytes(&mut server_key_1);
        if let Err(e) = stream.write_all(&server_key_1).await {
            error!("Can't write server key 1 on socket {:?}: {:?}", addr, e);
            return Err(Error::Io(e));
        }
        debug!("Send server key 1 on socket {:?}", addr);

        if let Err(e) = stream.read_exact(&mut client_key_2).await {
            error!("Can't read client key 2 on socket {:?}: {:?}", addr, e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 2 on socket {:?}", addr);

        OsRng.fill_bytes(&mut server_key_2);
        if let Err(e) = stream.write_all(&server_key_2).await {
            error!("Can't write server key 2 on socket {:?}: {:?}", addr, e);
            return Err(Error::Io(e));
        }
        debug!("Send server key 2 on socket {:?}", addr);

        Ok(CryptSession::new(
            [client_key_1, client_key_2],
            [server_key_1, server_key_2],
        ))
    }

    /// Reads the message from the global world message and returns the UID.
    async fn parse_uid(message: Option<Arc<Event>>) -> Result<u64> {
        match message {
            Some(event) => match &*event {
                Event::ResponseRegisterConnection { uid } => Ok(*uid),
                _ => Err(Error::WrongEventReceived),
            },
            None => Err(Error::NoSenderWaitingUid),
        }
    }

    /// Handles the writing / sending on the TCP stream.
    pub async fn handle_connection(&mut self) -> Result<()> {
        let mut header_buf = vec![0u8; 4];
        loop {
            tokio::select! {
                // RX
                result = self.stream.peek(&mut header_buf) => {
                   match result {
                       Ok(read_bytes) => {
                            if read_bytes == 4 {
                                self.stream.read_exact(&mut header_buf).await?;
                                self.cipher.crypt_client_data(&mut header_buf);
                                let packet_length = LittleEndian::read_u16(&header_buf[0..2]) as usize - 4;
                                let opcode = LittleEndian::read_u16(&header_buf[2..4]) as usize;
                                let mut data_buf = vec![0u8; packet_length];
                                if packet_length != 0 {
                                    self.stream.read_exact(&mut data_buf).await?;
                                    self.cipher.crypt_client_data(&mut data_buf);
                                    trace!("Received packet with opcode value {} on socket {:?}: {:?}", opcode, self.addr, data_buf);
                                }
                                if let Err(e) = self.handle_packet(opcode, data_buf).await {
                                    match e {
                                        Error::ConnectionClosed { .. } => {
                                            return Ok(());
                                        },
                                        _ => {
                                            return Err(e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return Err(Error::Io(e));
                        }
                    }
                }
                // TX
                message = self.global_response_channel.recv() => {
                    self.handle_message(message).await?;
                }
                // TODO Query instance response channel
            }
        }
    }

    /// Handles the incoming messages that could contain Response events or normal events.
    async fn handle_message(&mut self, message: Option<Arc<Event>>) -> Result<()> {
        match message {
            Some(event) => {
                if let Event::ResponseDropConnection { .. } = &*event {
                    return Err(Error::ConnectionClosed);
                }
                match event.data()? {
                    Some(data) => match event.opcode() {
                        Some(opcode) => {
                            self.send_packet(opcode, data).await?;
                        }
                        None => {
                            error!("Can't find opcode in event {:?} on socket {:?}", event, self.addr);
                        }
                    },
                    None => {
                        error!("Can't find data in event {:?} on socket {:?}", event, self.addr);
                    }
                }
            }
            None => {
                return Err(Error::NoSenderResponseChannel);
            }
        }
        Ok(())
    }

    /// Send packet to client.
    async fn send_packet(&mut self, opcode: Opcode, data: Vec<u8>) -> Result<()> {
        match self.reverse_opcode_table.get(&opcode) {
            Some(opcode_value) => {
                let len = data.len() + 4;
                if len > std::u16::MAX as usize {
                    error!(
                        "Length of packet {:?} too big for u16 length ({}) on socket {}",
                        opcode, len, self.addr
                    );
                } else {
                    self.stream.write_u16(len as u16).await?;
                    self.stream.write_u16(*opcode_value).await?;
                    self.stream.write(&data).await?;
                }
            }
            None => {
                error!(
                    "Can't find opcode {} in reverse mapping on socket {}",
                    opcode, self.addr
                );
            }
        }
        Ok(())
    }

    /// Decodes a packet from the given `Vec<u8>` and sends it to game server logic.
    async fn handle_packet(&mut self, opcode: usize, packet_data: Vec<u8>) -> Result<()> {
        let opcode_type = self.opcode_table[opcode];
        match opcode_type {
            Opcode::UNKNOWN => {
                warn!(
                    "Unmapped and unhandled packet with opcode value {} on socket {:?}",
                    opcode, self.addr
                );
            }
            _ => match Event::new_from_packet(self.uid, opcode_type, packet_data) {
                Ok(event) => {
                    debug!("Received valid packet {:?} on socket {:?}", opcode_type, self.addr);
                    // TODO test if the packet needs to be send to the global or the local ecs.
                    self.global_request_channel.send(Arc::new(event)).await?;
                }
                Err(e) => match e {
                    Error::NoEventMappingForPacket => {
                        warn!(
                            "No mapping found for packet {:?} on socket {:?}",
                            opcode_type, self.addr
                        );
                    }
                    _ => error!(
                        "Can't create event from valid packet {:?} on socket {:?}: {:?}",
                        opcode_type, self.addr, e
                    ),
                },
            },
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
