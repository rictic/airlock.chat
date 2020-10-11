use futures::join;
use std::net::SocketAddr;
use tokio::fs::File;
mod server;
use crate::server::Server;
use std::error::Error;
use std::path::Path;
use tokio_util::codec::{BytesCodec, FramedRead};

use std::convert::Infallible;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server as HyperServer, StatusCode};

async fn hello(req: Request<Body>) -> Result<Response<Body>, Box<dyn Error + Send + Sync>> {
  println!("Incoming request to {}", req.uri());
  let base_path = Path::new("../www/dist/");
  let req_path = if req.uri().path() == "/" {
    base_path.join("index.html")
  } else {
    base_path.join(&req.uri().path()[1..])
  };
  if !req_path.starts_with(base_path) {
    return Ok(not_found());
  }

  let mut builder = Response::builder().status(StatusCode::OK);

  if let Some(ext) = req_path.extension() {
    if ext == "wasm" {
      builder = builder.header("Content-Type", "application/wasm");
    }
  }

  if let Ok(file) = File::open(req_path).await {
    let stream = FramedRead::new(file, BytesCodec::new());
    let body = Body::wrap_stream(stream);
    return Ok(builder.body(body)?);
  }

  Ok(not_found())
}

fn not_found() -> Response<Body> {
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body("File not found".into())
    .unwrap()
}

pub async fn serve_static_files(addr: SocketAddr) -> Result<(), Box<dyn Error + Send + Sync>> {
  // For every connection, we must make a `Service` to handle all
  // incoming HTTP requests on said connection.
  let make_svc = make_service_fn(|_conn| {
    // This is the `Service` that will handle the connection.
    // `service_fn` is a helper to convert a function that
    // returns a Response into a `Service`.
    async { Ok::<_, Infallible>(service_fn(hello)) }
  });

  let server = HyperServer::bind(&addr).serve(make_svc);

  println!("Listening for static file traffic on http://{}", addr);

  server.await?;

  Ok(())
}

async fn serve_websocket(addr: SocketAddr) {
  let mut server = Server::new(addr).await.unwrap();
  println!("Listening for websocket traffic on: {}", addr);
  server.serve().await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let path = Path::new("../www/dist");
  if !path.exists() {
    return Err("Web directory isn't built. Be sure to do (cd www && npm run build) first.".into());
  }
  let f1 = tokio::spawn(serve_websocket(([0, 0, 0, 0], 3012).into()));
  let f2 = tokio::spawn(serve_static_files(([0, 0, 0, 0], 80).into()));
  let (r1, r2) = join!(f1, f2);
  r1?;
  match r2? {
    Ok(_) => (),
    Err(e) => println!("Error: {}", e),
  }
  Ok(())
}
