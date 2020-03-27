// We only use async code in the network related code.
// ECS are highly threaded and we use channels to
// do the communication anyhow, so only the network
// stuff needs to run in an async environment.
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    Ok(())
}
