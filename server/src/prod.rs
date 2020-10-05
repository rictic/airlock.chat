mod server;

use crate::server::Server;
use futures::join;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server as HyperServer, StatusCode};
use rust_us_core::UUID;
use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let path = Arc::new(Path::new("../www/dist"));
  if !path.exists() {
    return Err(
      r"Web directory isn't built. Be sure to do (cd www && npm run build) first.
Also this server must be run from the server directory."
        .into(),
    );
  }
  let f1 = tokio::spawn(serve_websocket(([0, 0, 0, 0], 3012).into()));
  let f2 = tokio::spawn(serve_static_files(([0, 0, 0, 0], 80).into(), path.clone()));
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
  path: Arc<&'static Path>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
  // We assume that our files don't change, and that we're the only webserver instance,
  // so this is easy mode for caching. We assume that all files change every time the
  // server restarts, and they never change otherwise. Both reasonable assumptions.
  let uuid = Arc::new(format!("{}", UUID::random()));
  // For every connection, we must make a `Service` to handle all
  // incoming HTTP requests on said connection.
  let server = HyperServer::bind(&addr).serve(make_service_fn(move |_conn| {
    // This is the `Service` that will handle the connection.
    // `service_fn` is a helper to convert a function that
    // returns a Response into a `Service`.
    let path = path.clone();
    let uuid = uuid.clone();
    async move {
      Ok::<_, Infallible>(service_fn(move |r| {
        serve_static_file(r, uuid.clone(), path.clone())
      }))
    }
  }));

  println!("Listening for static file traffic on http://{}", addr);
  server.await?;

  Ok(())
}

// Serve a single request for a static file.
async fn serve_static_file(
  req: Request<Body>,
  version: Arc<String>,
  base_path: Arc<&'static Path>,
) -> Result<Response<Body>, Box<dyn Error + Send + Sync>> {
  println!("Incoming request to {}", req.uri());
  let req_path = if req.uri().path() == "/" {
    base_path.join("index.html")
  } else {
    base_path.join(&req.uri().path()[1..])
  };
  if !req_path.starts_with(*base_path) {
    return Ok(not_found());
  }
  if let Some(req_etag) = req.headers().get(http::header::IF_NONE_MATCH) {
    if &*version == req_etag {
      return Ok(
        Response::builder()
          .status(StatusCode::NOT_MODIFIED)
          .body("".into())
          .unwrap(),
      );
    }
  }

  let mut builder = Response::builder()
    .status(StatusCode::OK)
    .header(http::header::ETAG, &*version);

  let content_type = match req_path.extension().map(|ext| ext.to_str()).or(None) {
    Some(Some("wasm")) => "application/wasm",
    Some(Some("html")) => "text/html",
    Some(Some("js")) => "text/javascript",
    _ => {
      // A file we shouldn't be serving right now.
      return Ok(
        Response::builder()
          .status(StatusCode::BAD_REQUEST)
          .body("We only serve out wasm, html, and js for now.".into())
          .unwrap(),
      );
    }
  };
  builder = builder.header(http::header::CONTENT_TYPE, content_type);

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
