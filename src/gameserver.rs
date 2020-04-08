/// The module that holds the game server logic.
use std::collections::HashMap;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tracing::{error, info, info_span, warn};
use tracing_futures::Instrument;

use crate::ecs::event::Event;
use crate::protocol::opcode::Opcode;
use crate::protocol::GameSession;
use crate::Result;

/// Main loop for the game server
pub async fn run(
    global_channel: Sender<Arc<Event>>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
) -> Result<()> {
    info!("listening on tcp://127.0.0.1:10001");
    let mut listener = TcpListener::bind("127.0.0.1:10001").await?;

    let arc_map = Arc::new(map);
    let arc_reverse_map = Arc::new(reverse_map);

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                let thread_channel = global_channel.clone();
                let thread_opcode_map = arc_map.clone();
                let thread_reverse_map = arc_reverse_map.clone();

                tokio::spawn(async move {
                    let span = info_span!("socket", %addr);
                    let _enter = span.enter();

                    info!("Incoming connection");
                    match GameSession::new(&mut socket, thread_channel, thread_opcode_map, thread_reverse_map).await {
                        Ok(mut session) => {
                            let connection = session.connection;
                            match session
                                .handle_connection()
                                .instrument(info_span!("connection", connection = %connection))
                                .await
                            {
                                Ok(_) => info!("Closed connection"),
                                Err(e) => warn!("Error while handling game session: {:?}", e),
                            }
                        }
                        Err(e) => error!("Failed create game session: {:?}", e),
                    }
                });
            }
            Err(e) => error!("Failed to open connection: {:?}", e),
        }
    }
}
