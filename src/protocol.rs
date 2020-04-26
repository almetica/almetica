/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::collections::HashMap;

use async_std::io::timeout;
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::sync::{channel, Receiver, Sender};
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use rand::rngs::OsRng;
use rand_core::RngCore;
use shipyard::EntityId;
use tracing::{debug, error, info, trace, warn};

use crate::crypt::CryptSession;
use crate::ecs::event::{EcsEvent, Event, EventTarget};
use crate::protocol::opcode::Opcode;
use crate::*;
use std::time::Duration;

enum ConnectionHandleEvent {
    Rx(usize),
    GlobalTx(Option<EcsEvent>),
    #[allow(dead_code)]
    LocalTx(Option<EcsEvent>), // FIXME remove me once implemented
}

/// Abstracts the game network protocol session.
pub struct GameSession<'a> {
    pub connection_id: EntityId,
    stream: &'a mut TcpStream,
    cipher: CryptSession,
    opcode_table: Arc<Vec<Opcode>>,
    reverse_opcode_table: Arc<HashMap<Opcode, u16>>,
    // Sending channel TO the global world
    global_request_channel: Sender<EcsEvent>,
    // Receiving channel FROM the global world
    global_response_channel: Receiver<EcsEvent>,
    // Sending channel TO the instance world
    _instance_request_channel: Option<Sender<EcsEvent>>,
    // Receiving channel FROM the instance world
    _instance_response_channel: Option<Receiver<EcsEvent>>,
    write_timeout_dur: Duration,
    read_timeout_dur: Duration,
    peek_timeout_dur: Duration,
}

impl<'a> GameSession<'a> {
    /// Initializes and returns a `GameSession` object.
    pub async fn new(
        stream: &'a mut TcpStream,
        global_request_channel: Sender<EcsEvent>,
        opcode_table: Arc<Vec<Opcode>>,
        reverse_opcode_table: Arc<HashMap<Opcode, u16>>,
    ) -> Result<GameSession<'a>> {
        // Initialize the stream cipher with the client.
        let cipher = GameSession::init_crypto(stream).await?;

        // Channel to receive response events from the global world ECS.
        let (tx_response_channel, rx_response_channel) = channel(128);
        global_request_channel
            .send(Arc::new(Event::RequestRegisterConnection {
                connection_id: None,
                response_channel: tx_response_channel,
            }))
            .await;

        // Wait for the global ECS to return an uid for the connection.
        let message = rx_response_channel.recv().await;
        let connection_id = GameSession::parse_connection(message).await?;

        info!(
            "Game session initialized under entity ID {:?}",
            connection_id
        );

        Ok(GameSession {
            connection_id,
            stream,
            cipher,
            opcode_table,
            reverse_opcode_table,
            global_request_channel,
            global_response_channel: rx_response_channel,
            _instance_request_channel: None,
            _instance_response_channel: None,
            write_timeout_dur: Duration::from_secs(15),
            read_timeout_dur: Duration::from_secs(15),
            peek_timeout_dur: Duration::from_secs(120),
        })
    }

    async fn init_crypto(stream: &mut TcpStream) -> Result<CryptSession> {
        let timeout_dur = Duration::from_secs(5);

        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word");
        if let Err(e) = timeout(timeout_dur, stream.write_all(&magic_word_buffer)).await {
            error!("Can't send magic word: {:?}", e);
            return Err(Error::Io(e));
        }

        if let Err(e) = timeout(timeout_dur, stream.read_exact(&mut client_key_1)).await {
            error!("Can't read client key 1: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 1");

        OsRng.fill_bytes(&mut server_key_1);
        if let Err(e) = timeout(timeout_dur, stream.write_all(&server_key_1)).await {
            error!("Can't write server key 1: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Send server key 1");

        if let Err(e) = timeout(timeout_dur, stream.read_exact(&mut client_key_2)).await {
            error!("Can't read client key 2: {:?}", e);
            return Err(Error::Io(e));
        }
        debug!("Received client key 2");

        OsRng.fill_bytes(&mut server_key_2);
        if let Err(e) = timeout(timeout_dur, stream.write_all(&server_key_2)).await {
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
    async fn parse_connection(message: Option<EcsEvent>) -> Result<EntityId> {
        match message {
            Some(event) => match &*event {
                Event::ResponseRegisterConnection { connection_id } => {
                    if let Some(entity) = connection_id {
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
        let mut header_buf = vec![0u8; 4];
        let mut peek_buf = vec![0u8; 4];

        loop {
            // TODO Query instance response channel

            let rx = async {
                let read = timeout(self.peek_timeout_dur, self.stream.peek(&mut peek_buf)).await?;
                Ok::<_, Error>(ConnectionHandleEvent::Rx(read))
            };

            let global_tx = async {
                let event = self.global_response_channel.recv().await;
                Ok::<_, Error>(ConnectionHandleEvent::GlobalTx(event))
            };

            match rx.race(global_tx).await? {
                ConnectionHandleEvent::Rx(read) => {
                    if read == 0 {
                        // Connection was closed
                        return Ok(());
                    }
                    if read == 4 {
                        timeout(
                            self.read_timeout_dur,
                            self.stream.read_exact(&mut header_buf),
                        )
                        .await?;
                        self.cipher.crypt_client_data(&mut header_buf);
                        let packet_length = LittleEndian::read_u16(&header_buf[0..2]) as usize - 4;
                        let opcode = LittleEndian::read_u16(&header_buf[2..4]) as usize;
                        let mut data_buf = vec![0u8; packet_length];
                        if packet_length != 0 {
                            timeout(self.read_timeout_dur, self.stream.read_exact(&mut data_buf))
                                .await?;
                            self.cipher.crypt_client_data(&mut data_buf);
                            trace!(
                                "Received packet with opcode value {}: {:?}",
                                opcode,
                                data_buf
                            );
                        }
                        if let Err(e) = self.handle_packet(opcode, data_buf).await {
                            match e {
                                Error::ConnectionClosed { .. } => {
                                    return Ok(());
                                }
                                _ => {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
                ConnectionHandleEvent::GlobalTx(event) => self.handle_message(event).await?,
                ConnectionHandleEvent::LocalTx(event) => self.handle_message(event).await?,
            };
        }
    }

    /// Handles the incoming messages that could contain Response events or normal events.
    async fn handle_message(&mut self, message: Option<EcsEvent>) -> Result<()> {
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
    async fn send_packet(&mut self, opcode: Opcode, mut data: Vec<u8>) -> Result<()> {
        match self.reverse_opcode_table.get(&opcode) {
            Some(opcode_value) => {
                let len = data.len() + 4;
                if len > std::u16::MAX as usize {
                    error!(
                        "Length of packet {:?} too big for u16 length ({}). Dropping packet.",
                        opcode, len
                    );
                } else {
                    let mut buffer = Vec::with_capacity(4 + data.len());
                    WriteBytesExt::write_u16::<LittleEndian>(&mut buffer, len as u16)?;
                    WriteBytesExt::write_u16::<LittleEndian>(&mut buffer, *opcode_value)?;
                    buffer.append(&mut data);

                    self.cipher.crypt_server_data(buffer.as_mut_slice());
                    timeout(self.write_timeout_dur, self.stream.write_all(&buffer)).await?;
                }
            }
            None => {
                error!(
                    "Can't find opcode {:?} in reverse mapping. Dropping packet.",
                    opcode
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
                warn!("Unmapped and unhandled packet with opcode value {}", opcode);
            }
            _ => match Event::new_from_packet(self.connection_id, opcode_type, packet_data) {
                Ok(event) => {
                    debug!("Received valid packet {:?}", opcode_type);
                    match event.target() {
                        EventTarget::Global => {
                            self.global_request_channel.send(Arc::new(event)).await;
                        }
                        EventTarget::Local => {
                            // TODO send to the local world
                        }
                        EventTarget::Connection => {
                            error!(
                                "Can't send event {} with target Connection from a connection",
                                event
                            );
                        }
                    }
                }
                Err(e) => match e {
                    Error::NoEventMappingForPacket => {
                        warn!("No mapping found for packet {:?}", opcode_type);
                    }
                    _ => error!(
                        "Can't create event from valid packet {:?}: {:?}",
                        opcode_type, e
                    ),
                },
            },
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};

    use async_std::future::timeout;
    use async_std::net::{TcpListener, TcpStream};
    use async_std::sync::channel;
    use async_std::task::{self, JoinHandle};
    use byteorder::{ByteOrder, LittleEndian};
    use shipyard::*;

    use crate::dataloader::*;
    use crate::ecs::component::Connection;
    use crate::ecs::event::Event::{RequestRegisterConnection, ResponseRegisterConnection};
    use crate::protocol::opcode::Opcode;
    use crate::protocol::protocol::GameSession;
    use crate::Result;

    use super::*;
    use shipyard::EntityId;

    async fn get_opcode_tables() -> Result<(Vec<Opcode>, HashMap<Opcode, u16>)> {
        let mut file = Vec::new();
        file.write_all(
            "
        C_CHECK_VERSION: 1
        S_CHECK_VERSION: 2
        "
            .as_bytes(),
        )
        .await?;

        let table = read_opcode_table(&mut file.as_slice())?;
        let reverse_map = calculate_reverse_map(table.as_slice());

        Ok((table, reverse_map))
    }

    fn get_new_entity_with_connection_component() -> EntityId {
        let world = World::new();

        world.run(
            |mut entities: EntitiesViewMut, mut connections: ViewMut<Connection>| {
                entities.add_entity(
                    &mut connections,
                    Connection {
                        verified: false,
                        version_checked: false,
                        region: None,
                        last_pong: Instant::now(),
                        waiting_for_pong: false,
                    },
                )
            },
        )
    }

    async fn spawn_dummy_server() -> Result<(SocketAddr, JoinHandle<()>, JoinHandle<()>)> {
        let srv = TcpListener::bind("127.0.0.1:0").await?;
        let addr = srv.local_addr()?;
        let (opcode_mapping, reverse_opcode_mapping) = get_opcode_tables().await?;
        let (tx_channel, rx_channel) = channel(1024);

        // TCP server
        let tcp_join = task::spawn(async move {
            let (mut socket, _) = srv.accept().await.unwrap();
            let _session = GameSession::new(
                &mut socket,
                tx_channel,
                Arc::new(opcode_mapping),
                Arc::new(reverse_opcode_mapping),
            )
            .await
            .unwrap();
        });

        // World loop mock
        let world_join = task::spawn(async move {
            let connection_id = Some(get_new_entity_with_connection_component());
            loop {
                task::yield_now().await;
                if let Some(event) = rx_channel.recv().await {
                    match &*event {
                        RequestRegisterConnection {
                            response_channel, ..
                        } => {
                            let tx = response_channel.clone();
                            tx.send(Arc::new(ResponseRegisterConnection { connection_id }))
                                .await;
                            break;
                        }
                        _ => break,
                    }
                }
            }
        });

        Ok((addr, tcp_join, world_join))
    }

    #[async_std::test]
    async fn test_gamesession_creation() -> Result<()> {
        let (addr, tcp_join, world_join) = spawn_dummy_server().await?;
        let mut stream = TcpStream::connect(&addr).await?;

        // hello stage
        let mut hello_buffer = vec![0u8; 4];
        stream.read_exact(hello_buffer.as_mut_slice()).await?;

        let hello = LittleEndian::read_u16(&hello_buffer[0..4]) as u32;
        assert_eq!(hello, 1);

        // key exchange stage
        let mut client_key1 = vec![0u8; 128];
        let mut client_key2 = vec![0u8; 128];
        let mut server_key1 = vec![0u8; 128];
        let mut server_key2 = vec![0u8; 128];

        OsRng.fill_bytes(&mut client_key1);
        OsRng.fill_bytes(&mut client_key2);

        if let Err(e) = timeout(
            Duration::from_millis(100),
            stream.write_all(client_key1.as_mut_slice()),
        )
        .await
        {
            panic!("{}", e);
        }

        if let Err(e) = timeout(
            Duration::from_millis(100),
            stream.read_exact(server_key1.as_mut_slice()),
        )
        .await
        {
            panic!("{}", e);
        }

        if let Err(e) = timeout(
            Duration::from_millis(100),
            stream.write_all(client_key2.as_mut_slice()),
        )
        .await
        {
            panic!("{}", e);
        }

        if let Err(e) = timeout(
            Duration::from_millis(100),
            stream.read_exact(server_key2.as_mut_slice()),
        )
        .await
        {
            panic!("{}", e);
        }

        tcp_join.await;
        world_join.await;
        Ok(())
    }
}
