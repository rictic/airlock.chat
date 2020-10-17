mod server;
use crate::server::{client_connected, WebsocketServer};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use warp::Filter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr: SocketAddr = ([0, 0, 0, 0], 3012).into();
    let gameserver: Arc<Mutex<WebsocketServer>> = Arc::default();
    let gameserver = warp::any().map(move || gameserver.clone());
    let websocket_server = warp::ws()
        .and(gameserver)
        .map(|ws: warp::ws::Ws, gameserver| {
            ws.on_upgrade(move |socket| client_connected(socket, gameserver))
        });

    let server = warp::serve(websocket_server);
    println!("Listening on: {}", addr);
    server.run(addr).await;

    Ok(())
}
