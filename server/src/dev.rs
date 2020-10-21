#![warn(rust_2018_idioms)]

mod server;
use crate::server::{client_connected, WebsocketServer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::{error::Error, path::PathBuf};
use warp::Filter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let site_data_path: PathBuf = "../site_data".into();
  if !site_data_path.exists() {
    return Err(format!("Expected to find site_data dir at {:?}", site_data_path).into());
  }
  let addr: SocketAddr = ([0, 0, 0, 0], 3012).into();
  let gameserver = Arc::new(Mutex::new(WebsocketServer::new(site_data_path)?));
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
