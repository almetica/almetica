/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::collections::HashMap;

use crate::crypt::CryptSession;
use crate::ecs::event::{Event, EventTarget};
use crate::*;
use opcode::Opcode;

use byteorder::{ByteOrder, LittleEndian};
use legion::entity::Entity;
use rand::rngs::OsRng;
use rand_core::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{debug, error, info, info_span, trace, warn};

/// Abstracts the game network protocol session.
pub struct GameSession<'a> {
    connection: Entity,
    stream: &'a mut TcpStream,
    cipher: CryptSession,
    opcode_table: Arc<Vec<Opcode>>,
    reverse_opcode_table: Arc<HashMap<Opcode, u16>>,
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
        mut global_request_channel: Sender<Arc<Event>>,
        opcode_table: Arc<Vec<Opcode>>,
        reverse_opcode_table: Arc<HashMap<Opcode, u16>>,
    ) -> Result<GameSession<'a>> {
        // Initialize the stream cipher with the client.
        let cipher = GameSession::init_crypto(stream).await?;

        // Channel to receive response events from the global world ECS.
        let (tx_response_channel, mut rx_response_channel) = channel(128);
        global_request_channel
            .send(Arc::new(Event::RequestRegisterConnection {
                connection: None,
                response_channel: tx_response_channel,
            }))
            .await?;
        // Wait for the global ECS to return an uid for the connection.
        let message = rx_response_channel.recv().await;
        let connection = GameSession::parse_connection(message).await?;

        info!("Game session initialized under entity ID {}", connection);

        Ok(GameSession {
            connection,
            stream,
            cipher,
            opcode_table,
            reverse_opcode_table,
            global_request_channel,
            global_response_channel: rx_response_channel,
            _instance_request_channel: None,
            _instance_response_channel: None,
        })
    }

    async fn init_crypto(stream: &mut TcpStream) -> Result<CryptSession> {
        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word");
        if let Err(e) = stream.write_all(&magic_word_buffer).await {
            error!("Can't send magic word: {:?}", e);
            return Err(Error::Io(e));
        }

        if let Err(e) = stream.read_exact(&mut client_key_1).await {
            error!("Can't read client key 1: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 1");

        OsRng.fill_bytes(&mut server_key_1);
        if let Err(e) = stream.write_all(&server_key_1).await {
            error!("Can't write server key 1: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Send server key 1");

        if let Err(e) = stream.read_exact(&mut client_key_2).await {
            error!("Can't read client key 2: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 2");

        OsRng.fill_bytes(&mut server_key_2);
        if let Err(e) = stream.write_all(&server_key_2).await {
            error!("Can't write server key 2: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Send server key 2");

        Ok(CryptSession::new(
            [client_key_1, client_key_2],
            [server_key_1, server_key_2],
        ))
    }

    /// Reads the message from the global world message and returns the connection.
    async fn parse_connection(message: Option<Arc<Event>>) -> Result<Entity> {
        match message {
            Some(event) => match &*event {
                Event::ResponseRegisterConnection { connection } => {
                    if let Some(entity) = connection {
                        Ok(*entity)
                    } else {
                        Err(Error::EntityNotSet)
                    }
                }
                _ => Err(Error::WrongEventReceived),
            },
            None => Err(Error::NoSenderWaitingConnectionEntity),
        }
    }

    /// Handles the writing / sending on the TCP stream.
    pub async fn handle_connection(&mut self) -> Result<()> {
        let span = info_span!("connection", connection = %self.connection);
        let _enter = span.enter();

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
                                    trace!("Received packet with opcode value {}: {:?}", opcode, data_buf);
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
                            debug!("Sending packet {:?}", opcode);
                            trace!("Packet data: {:?}", data);
                            self.send_packet(opcode, data).await?;
                        }
                        None => {
                            error!("Can't find opcode in event {:?}", event);
                        }
                    },
                    None => {
                        error!("Can't find data in event {:?}", event);
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
                    error!("Length of packet {:?} too big for u16 length ({})", opcode, len);
                } else {
                    self.stream.write_u16(len as u16).await?;
                    self.stream.write_u16(*opcode_value).await?;
                    self.stream.write(&data).await?;
                }
            }
            None => {
                error!("Can't find opcode {:?} in reverse mapping", opcode);
            }
        }
        Ok(())
    }

    /// Decodes a packet from the given `Vec<u8>` and sends it to game server logic.
    async fn handle_packet(&mut self, opcode: usize, packet_data: Vec<u8>) -> Result<()> {
        let opcode_type = self.opcode_table[opcode];
        match opcode_type {
            Opcode::UNKNOWN => {
                warn!("Unmapped and unhandled packet with opcode value {}", opcode);
            }
            _ => match Event::new_from_packet(self.connection, opcode_type, packet_data) {
                Ok(event) => {
                    debug!("Received valid packet {:?}", opcode_type);
                    match event.target() {
                        EventTarget::Global => {
                            self.global_request_channel.send(Arc::new(event)).await?;
                        }
                        EventTarget::Local => {
                            // TODO send to the local world
                        }
                        EventTarget::Connection => {
                            error!("Can't send event {} with target Connection from a connection", event);
                        }
                    }
                }
                Err(e) => match e {
                    Error::NoEventMappingForPacket => {
                        warn!("No mapping found for packet {:?}", opcode_type);
                    }
                    _ => error!("Can't create event from valid packet {:?}: {:?}", opcode_type, e),
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
