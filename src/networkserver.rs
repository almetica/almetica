/// The module of the network server that handles the TCP connections to the clients.
use crate::config::Configuration;
use crate::ecs::message::EcsMessage;
use crate::protocol::opcode::Opcode;
use crate::protocol::GameSession;
use crate::{AlmeticaError, Result};
use async_std::net::TcpListener;
use async_std::sync::Sender;
use async_std::task;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, info_span, warn};
use tracing_futures::Instrument;

/// Main loop for the network server
pub async fn run(
    global_channel: Sender<EcsMessage>,
    map: Vec<Opcode>,
    reverse_map: HashMap<Opcode, u16>,
    config: Configuration,
) -> Result<()> {
    let listen_string = format!("{}:{}", config.server.ip, config.server.game_port);
    info!("listening on tcp://{}", listen_string);
    let listener = TcpListener::bind(listen_string).await?;

    let arc_map = Arc::new(map);
    let arc_reverse_map = Arc::new(reverse_map);

    loop {
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                let thread_channel = global_channel.clone();
                let thread_opcode_map = arc_map.clone();
                let thread_reverse_map = arc_reverse_map.clone();

                task::spawn(
                    async move {
                        info!("Incoming connection");
                        match GameSession::new(
                            &mut socket,
                            thread_channel,
                            thread_opcode_map,
                            thread_reverse_map,
                        )
                        .await
                        {
                            Ok(mut session) => {
                                let connection_global_world_id = session.connection_global_world_id;
                                match session
                                    .handle_connection()
                                    .instrument(
                                        info_span!("connection_global_world_id", connection_global_world_id = ?connection_global_world_id),
                                    )
                                    .await
                                {
                                    Ok(_) => info!("Connection closed"),
                                    Err(e) => match e.downcast_ref::<AlmeticaError>() {
                                        Some(AlmeticaError::ConnectionClosed) => {
                                            info!("Connection closed");
                                        }
                                        Some(..) | None => {
                                            warn!("Error while handling game session: {:?}", e)
                                        }
                                    },
                                }
                            }
                            Err(e) => error!("Failed create game session: {:?}", e),
                        }
                    }
                    .instrument(info_span!("socket", %addr)),
                );
            }
            Err(e) => error!("Failed to open connection: {:?}", e),
        }
    }
}
