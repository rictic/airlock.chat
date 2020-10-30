#![warn(rust_2018_idioms)]

mod server;

use crate::server::game_server;
use futures::join;
use std::path::Path;
use std::str::FromStr;
use std::{error::Error, path::PathBuf};
use warp::Filter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let path = Path::new("./server/dist");
  if !path.exists() {
    return Err(
      r"Web directory isn't built. Be sure to do (cd www && npm run build) first.
Also this server must be run from the server directory."
        .into(),
    );
  }
  let site_data_path: PathBuf = "../site_data".into();
  if !site_data_path.exists() {
    return Err(format!("Could not find site_data directory at {:?}", site_data_path).into());
  }

  // Define the static file server.
  // TODO when I'm smarter, figure out to compress with the best available
  // compression algorithm.
  let fileserver = warp::fs::dir(path).with(warp::compression::gzip());

  // If an incoming request looks like a websockets request, serve it as one,
  // otherwise treat it as a request for a static file
  let server = game_server(site_data_path)?.or(fileserver);

  // Managed by certbot, see https://certbot.eff.org/lets-encrypt/debianbuster-other
  let privkey = Path::new("/etc/letsencrypt/live/airlock.chat/privkey.pem");
  let cert = Path::new("/etc/letsencrypt/live/airlock.chat/cert.pem");

  if privkey.exists() && cert.exists() {
    let tls_server = warp::serve(server)
      .tls()
      .cert_path(cert)
      .key_path(privkey)
      .run(([0, 0, 0, 0], 443));
    let http_server = warp::serve(warp::path::tail().map(move |path: warp::path::Tail| {
      warp::redirect::redirect(
        warp::http::Uri::from_str(&format!("https://airlock.chat/{}", path.as_str()))
          .expect("illegal URL path?"),
      )
    }))
    .run(([0, 0, 0, 0], 80));
    println!("Listening for static file traffic on https://airlock.chat/");
    join!(tls_server, http_server);
  } else {
    let server = warp::serve(server).run(([0, 0, 0, 0], 80));
    println!("Listening for static file traffic on http://localhost");
    server.await;
  }

  Ok(())
}
