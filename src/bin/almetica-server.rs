// We try to use as much async code as possible where it's possible.
// An exception to the rule: The ECS world threads.
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    Ok(())
}
