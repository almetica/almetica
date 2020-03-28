/// Module that implements the network protocol used by TERA.
pub mod opcode;
pub mod packet;
pub mod serde;

use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::crypt::CryptSession;
use super::*;
use log::{debug, error, info};
use rand::rngs::OsRng;
use rand_core::RngCore;

/// Abstracts the game network protocol session.
pub struct GameSession {
    _uid: Option<u64>, // User ID
    _addr: SocketAddr,
    _crypt: CryptSession,
    // TODO Will later have TX/RX channels to the event handler
}

impl GameSession {
    /// Initializes and returns a `GameSession` object.
    pub async fn new<T: ?Sized>(stream: &mut T, addr: SocketAddr) -> Result<GameSession>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let magic_word_buffer: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
        let mut client_key_1 = vec![0; 128];
        let mut client_key_2 = vec![0; 128];
        let mut server_key_1 = vec![0; 128];
        let mut server_key_2 = vec![0; 128];
        debug!("Sending magic word on socket: {}", addr);
        match stream.write_all(&magic_word_buffer).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't send magic word on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };

        match stream.read_exact(&mut client_key_1).await {
            Ok(_i) => 0,
            Err(e) => {
                error!("Can't read client key 1 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Received client key 1 on socket {}", addr);

        OsRng.fill_bytes(&mut server_key_1);
        match stream.write_all(&server_key_1).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 1 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 1 on socket {}", addr);

        match stream.read_exact(&mut client_key_2).await {
            Ok(_i) => 0,
            Err(e) => {
                error!("Can't read client key 2 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Received client key 2 on socket {}", addr);

        OsRng.fill_bytes(&mut server_key_2);
        match stream.write_all(&server_key_2).await {
            Ok(()) => (),
            Err(e) => {
                error!("Can't write server key 2 on socket {}: {}", addr, e);
                return Err(Error::Io(e));
            }
        };
        debug!("Send server key 2 on socket {}", addr);

        let cs = CryptSession::new([client_key_1, client_key_2], [server_key_1, server_key_2]);
        let gs = GameSession {
            _uid: None,
            _addr: addr,
            _crypt: cs,
        };

        info!("Game session initialized for socket: {}", addr);
        Ok(gs)
    }

    /// Handles the writing / sending on the TCP stream.
    pub async fn handle_connection(self) -> Result<()> {
        // TODO
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::test;

    // TODO get this test running again
    #[tokio::test]
    async fn test_read_gamesession_creation() -> Result<(), Error> {
        // Mocked TCP stream. Implementation below.
        let mut stream = StreamMock::default();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        GameSession::new(&mut stream, addr).await?;

        assert_eq!(4, stream.state);
        Ok(())
    }

    // We need to create a mock to abstract the TCP stream.
    struct StreamMock {
        pub state: i64,
    }

    impl Default for StreamMock {
        fn default() -> Self {
            StreamMock { state: -1 }
        }
    }

    impl AsyncRead for StreamMock {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &mut [u8],
        ) -> Poll<Result<usize, Error>> {
            match self.state {
                0 => {
                    self.state = 1;
                    let client_key1: [u8; 128] = [0xaa; 128];
                    buf.copy_from_slice(&client_key1);
                    Poll::Ready(Ok(client_key1.len()))
                }
                2 => {
                    self.state = 3;
                    let client_key2: [u8; 128] = [0xcc; 128];
                    buf.copy_from_slice(&client_key2);
                    Poll::Ready(Ok(client_key2.len()))
                }
                _ => Poll::Ready(Err(Error::new(
                    ErrorKind::Other,
                    format!("unexpected read at state {}", self.state),
                ))),
            }
        }
    }

    impl AsyncWrite for StreamMock {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, Error>> {
            match self.state {
                -1 => {
                    self.state = 0;
                    let mut magic_word: [u8; 4] = [0xff; 4];
                    magic_word.copy_from_slice(buf);
                    if magic_word[0] != 1 {
                        return Poll::Ready(Err(Error::new(
                            ErrorKind::Other,
                            format!("wrong magic word"),
                        )));
                    }
                    Poll::Ready(Ok(magic_word.len()))
                }
                1 => {
                    self.state = 2;
                    let mut server_key_1: [u8; 128] = [0xff; 128];
                    server_key_1.copy_from_slice(buf);
                    Poll::Ready(Ok(server_key_1.len()))
                }
                3 => {
                    self.state = 4;
                    let mut server_key_2: [u8; 128] = [0xff; 128];
                    server_key_2.copy_from_slice(buf);
                    Poll::Ready(Ok(server_key_2.len()))
                }
                _ => Poll::Ready(Err(Error::new(
                    ErrorKind::Other,
                    format!("unexpected write at state {}", self.state),
                ))),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
            Poll::Ready(Err(Error::new(
                ErrorKind::Other,
                format!("unexpected flush at state {}", self.state),
            )))
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Error>> {
            Poll::Ready(Err(Error::new(
                ErrorKind::Other,
                format!("unexpected shutdown at state {}", self.state),
            )))
        }
    }
}
