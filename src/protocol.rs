/// Module that implments the network protocol used by tera.
pub mod opcode;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

use super::*;
use super::crypt::CryptSession;
use log::{debug, info, error};
use rand::rngs::OsRng;
use rand_core::RngCore;

/// Abstracts the game network protocol session.
struct GameSession {
    uid: Option<u64>, // User ID
    addr: SocketAddr,
    crypt: CryptSession,
    // TODO Will later have TX/RX channels to the event handler
}

impl GameSession {
    /// Initializes and returns a `GameSession` object.
    pub fn new(stream: &mut TcpStream, addr: SocketAddr) -> Result<GameSession> {
        let mut magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1: [u8; 128] = [0; 128];
        let mut client_key_2: [u8; 128] = [0; 128];
        let mut server_key_1: [u8; 128] = [0; 128];
        let mut server_key_2: [u8; 128] = [0; 128];
        
        debug!("Sending magic word on socket: {}", addr);
        match stream.write_all(&magic_word_buffer) {
            Ok(()) => (),
            Err(e) => {
                error!("Can't send magic word on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };

        match stream.read_exact(&mut client_key_1) {
            Ok(()) => (),
            Err(e) => {
                error!("Can't read client key 1 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Recieved client key 1 on socket {}", addr);

        OsRng.fill_bytes(&mut server_key_1);
        match stream.write_all(&server_key_1) {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 1 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 1 on socket {}", addr);

        match stream.read_exact(&mut client_key_2) {
            Ok(()) => (),
            Err(e) => {
                error!("Can't read client key 2 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Recieved client key 2 on socket {}", addr);

        OsRng.fill_bytes(&mut server_key_2);
        match stream.write_all(&server_key_2) {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 2 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 2 on socket {}", addr);

        let cs = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);
        let gs = GameSession {
            uid: None,
            addr: addr,
            crypt: cs,
        };

        info!("Game session initialized for socket: {}", addr);
        Ok(gs)
    }

    /// Handles the writing / sending on the TCP stream.
    pub fn handle_connection(stream: &mut TcpStream) {
        // TODO
    }  
}

// TODO Write tests
