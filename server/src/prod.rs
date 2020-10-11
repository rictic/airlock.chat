mod server;

use crate::server::Server;
use futures::join;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let path_str = "./server/dist";
  let path = Path::new(path_str);
  if !path.exists() {
    return Err(
      r"Web directory isn't built. Be sure to do (cd www && npm run build) first.
Also this server must be run from the server directory."
        .into(),
    );
  }
  let f1 = tokio::spawn(serve_websocket(([0, 0, 0, 0], 3012).into()));
  let f2 = tokio::spawn(serve_static_files(([0, 0, 0, 0], 80).into(), path_str));
  let (r1, r2) = join!(f1, f2);
  r1?;
  match r2? {
    Ok(_) => (),
    Err(e) => println!("Error: {}", e),
  }
  Ok(())
}

// Run a websocket game server.
async fn serve_websocket(addr: SocketAddr) {
  let mut server = Server::new(addr).await.unwrap();
  println!("Listening for websocket traffic on: {}", addr);
  server.serve().await;
}

// Run a webserver that serves out static files.
pub async fn serve_static_files(
  addr: SocketAddr,
  path: &'static str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
  let server = warp::serve(warp::fs::dir(path)).run(addr);
  println!("Listening for static file traffic on http://{}", addr);
  server.await;

  Ok(())
}
