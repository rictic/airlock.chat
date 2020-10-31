use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use rust_us_core::{get_version_sha, GameRecordingWriter, ServerToClientMessage};
use rust_us_core::{Broadcaster, ClientToServerMessage, GameServer, GameStatus, UUID};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use std::{collections::HashMap, path::PathBuf};
use std::{error::Error, io::Write};
use tokio::time::delay_for;
use warp::{filters::BoxedFilter, ws::Message, Reply};
use warp::{ws::WebSocket, Filter};

type Tx = UnboundedSender<Message>;
type Room = Arc<Mutex<HashMap<UUID, Tx>>>;

// See the returning.rs example. This could be more efficient if we avoided
// the BoxedFilter part, but figuring out the types... whoo boy.
// https://github.com/seanmonstar/warp/blob/master/examples/returning.rs
pub fn game_server(site_data_path: PathBuf) -> Result<BoxedFilter<(impl Reply,)>, Box<dyn Error>> {
  // Define the websocket server
  let gameserver: Arc<Mutex<WebsocketServer>> =
    Arc::new(Mutex::new(WebsocketServer::new(site_data_path.clone())?));
  let gameserver = warp::any().map(move || gameserver.clone());
  let websocket_server = warp::ws()
    .and(gameserver)
    .map(|ws: warp::ws::Ws, gameserver| {
      ws.on_upgrade(move |socket| client_connected(socket, gameserver))
    });

  let save_server = warp::path("replay_file")
    .and(warp::filters::fs::dir(site_data_path.join("replays")));

  Ok(websocket_server.or(save_server).boxed())
}

#[derive(Clone)]
pub struct WebsocketServer {
  room: Room,
  game_server: Arc<Mutex<GameServer>>,
  site_data_path: PathBuf,
}

impl WebsocketServer {
  pub fn new(site_data_path: PathBuf) -> Result<Self, Box<dyn Error>> {
    let (room, game_server) = make_new_game(site_data_path.as_path())?;
    Ok(WebsocketServer {
      room,
      game_server,
      site_data_path,
    })
  }
}

struct BroadCastServer {
  room: Room,
}

impl Broadcaster for BroadCastServer {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
    println!("Broadcasting {:?}", message);
    let message = Message::text(serde_json::to_string(message)?);
    broadcast(self.room.clone(), &message);
    Ok(())
  }

  fn send_to_player(
    &self,
    uuid: &UUID,
    message: &ServerToClientMessage,
  ) -> Result<(), Box<dyn Error>> {
    let room = self.room.clone();
    let peers = room.lock().unwrap();
    let player_connection = match peers.get(uuid) {
      None => {
        // This could be a race condition with a player disconnecting.
        // Not a fatal error.
        println!("No player connection with UUID {:?}", uuid);
        return Ok(());
      }
      Some(p) => p,
    };
    println!("Sending {:?} to {:?}", message, uuid);
    let encoded = Message::text(serde_json::to_string(message)?);
    if let Err(e) = player_connection.unbounded_send(encoded) {
      println!("Error with unbounded send: {}", e);
    }
    Ok(())
  }
}

pub async fn client_connected(ws: WebSocket, ws_server: Arc<Mutex<WebsocketServer>>) {
  let game_server;
  let room;
  {
    let mut ws_server = ws_server.lock().unwrap();
    let prev_game_finished;
    {
      let game_server = ws_server.game_server.lock().unwrap();
      prev_game_finished = game_server.state.status.finished();
    }
    if prev_game_finished {
      // The previous game is finished. Create a new game and direct future players to it.
      let (room, game_server) = make_new_game(ws_server.site_data_path.as_path()).unwrap();
      ws_server.room = room;
      ws_server.game_server = game_server;
      println!("Starting a new game for the new client!");
    }
    game_server = ws_server.game_server.clone();
    room = ws_server.room.clone();
  }
  tokio::spawn(handle_connection(game_server, room, ws));
}

fn make_new_game(site_data_path: &Path) -> Result<(Room, Arc<Mutex<GameServer>>), Box<dyn Error>> {
  let game_id = UUID::random();
  let writer = get_game_recording_writer(site_data_path, game_id)?;
  let room = Room::default();
  let broadcast_server = BroadCastServer { room: room.clone() };
  let game_server = Arc::new(Mutex::new(GameServer::new(
    game_id,
    Box::new(broadcast_server),
    Some(writer),
  )));
  Ok((room, game_server))
}

fn get_game_recording_writer(
  site_data_path: &Path,
  game_id: UUID,
) -> Result<GameRecordingWriter<Box<dyn Write + Send>>, Box<dyn Error>> {
  let replay_dir =
    site_data_path.join(format!("replays/{:02x}/{:02x}", game_id.v[0], game_id.v[1]));
  if !replay_dir.exists() {
    std::fs::create_dir_all(&replay_dir)?;
  }
  let replay_path = replay_dir.join(format!("{}.airlockreplay", game_id));
  let replay_file = std::fs::File::create(replay_path)?;
  let inner_writer: std::boxed::Box<dyn std::io::Write + std::marker::Send> = Box::new(replay_file);
  Ok(GameRecordingWriter::new(
    inner_writer,
    get_version_sha(),
    game_id,
  )?)
}

async fn simulation_loop(game_server: Arc<Mutex<GameServer>>, room: Room) {
  let mut prev = Instant::now();
  loop {
    delay_for(Duration::from_millis(16)).await;
    let now = Instant::now();
    let elapsed = now - prev;
    prev = now;
    let mut game_server = game_server.lock().unwrap();
    // The server wants to disconnect the players (e.g. timeout),
    // so close all the connections.
    let finished = match game_server.simulate(elapsed) {
      Err(e) => {
        println!("Error handling simulation: {}", e);
        true
      }
      Ok(b) => b,
    };
    if finished {
      println!("Game finished, done simulating it on the server.");
      let mut room = room.lock().unwrap();
      for (_, player) in room.iter_mut() {
        player.disconnect();
      }
      break;
    }
  }
}

async fn handle_connection(game_server: Arc<Mutex<GameServer>>, room: Room, socket: WebSocket) {
  // Ensure we're simulating this game.
  {
    let mut game_server_unlocked = game_server.lock().unwrap();
    if game_server_unlocked.state.status == GameStatus::Connecting {
      game_server_unlocked.state.status = GameStatus::Lobby;
      tokio::spawn(simulation_loop(game_server.clone(), room.clone()));
    }
  }

  // Insert the write part of this peer to the peer map.
  let (tx, rx) = unbounded();
  let (outgoing, incoming) = socket.split();

  // Generate a connection id for our user.
  let uuid = game_server.lock().unwrap().get_uuid_for_new_connection();

  // Hook up the ability to send messages to this client.
  room.lock().unwrap().insert(uuid, tx);

  // Buffer and forward messages.
  let receive_from_others = rx.map(Ok).forward(outgoing);

  // Handle incoming messages from the client.
  let broadcast_incoming = incoming.try_for_each(|msg| {
    let message_text = match msg.to_str() {
      Ok(s) => s,
      Err(_) => return future::ok(()), // other kind of message, ignore
    };
    let message: ClientToServerMessage = match serde_json::from_str(&message_text) {
      Ok(m) => m,
      Err(e) => {
        println!("Unable to deserialize {:?} – {:?}", message_text, e);
        return future::ok(());
      }
    };
    let mut game_server = game_server.lock().unwrap();
    match game_server.handle_message(uuid, message) {
      Ok(_) => (),
      Err(e) => {
        println!("Failed to handle message from {:?}: {}", uuid, e);
        return future::ok(());
      }
    }
    future::ok(())
  });

  // Some magic incantation for futures to work.
  pin_mut!(broadcast_incoming, receive_from_others);

  // Wait for either side to close the connection
  future::select(broadcast_incoming, receive_from_others).await;

  println!("{} disconnected", uuid);
  room.lock().unwrap().remove(&uuid);

  let mut game_server = game_server.lock().unwrap();
  if game_server.state.status.finished() {
    // The game is done, and the simulation loop will clean up, just return.
  } else {
    // Disconnect this connection from the game server.
    match game_server.disconnected(uuid) {
      Ok(()) => (),
      Err(e) => println!("Error handling disconnection: {}", e),
    }
  }
}

fn broadcast(room: Room, msg: &Message) {
  let peers = room.lock().unwrap();
  for (_, recp) in peers.iter() {
    if let Err(e) = recp.unbounded_send(msg.clone()) {
      println!("Error with unbounded send: {}", e);
    }
  }
}
