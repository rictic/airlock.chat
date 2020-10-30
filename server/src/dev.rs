#![warn(rust_2018_idioms)]

mod server;
use crate::server::game_server;
use std::net::SocketAddr;
use std::{error::Error, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let site_data_path: PathBuf = "../site_data".into();
  if !site_data_path.exists() {
    return Err(format!("Expected to find site_data dir at {:?}", site_data_path).into());
  }
  let addr: SocketAddr = ([127, 0, 0, 1], 3012).into();
  let server = warp::serve(game_server(site_data_path)?);
  println!("Listening on: {}", addr);
  server.run(addr).await;

  Ok(())
}
