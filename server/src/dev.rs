mod server;
use crate::server::Server;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:3012".to_string();
    let mut server = Server::new(addr.clone()).await?;
    println!("Listening on: {}", addr);
    server.serve().await;

    Ok(())
}
