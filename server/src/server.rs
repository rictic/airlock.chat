use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use rust_us_core::ServerToClientMessage;
use rust_us_core::{Broadcaster, ClientToServerMessage, GameServer, GameStatus, UUID};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tokio::time::delay_for;
use warp::ws::Message;
use warp::ws::WebSocket;

type Tx = UnboundedSender<Message>;
type Room = Arc<Mutex<HashMap<UUID, Tx>>>;

#[derive(Clone)]
pub struct WebsocketServer {
  room: Room,
  game_server: Arc<Mutex<GameServer>>,
}

impl Default for WebsocketServer {
  fn default() -> Self {
    let room = Room::default();
    let game_server = Arc::new(Mutex::new(GameServer::new(Box::new(BroadCastServer {
      room: room.clone(),
    }))));
    WebsocketServer { room, game_server }
  }
}

impl WebsocketServer {}

struct BroadCastServer {
  room: Room,
}

impl Broadcaster for BroadCastServer {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
    println!("Broadcasting {:?}", message);
    broadcast(
      self.room.clone(),
      &Message::text(serde_json::to_string(message)?),
    )?;
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
      None => return Err(format!("No player connection with UUID {:?}", uuid).into()),
      Some(p) => p,
    };
    println!("Sending {:?} to {:?}", message, uuid);
    player_connection.unbounded_send(Message::text(serde_json::to_string(message)?))?;
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
      let room = Room::default();
      let broadcast_server = BroadCastServer { room: room.clone() };
      ws_server.room = room;
      ws_server.game_server = Arc::new(Mutex::new(GameServer::new(Box::new(broadcast_server))));
      println!("Starting a new game for the new client!");
    }
    game_server = ws_server.game_server.clone();
    room = ws_server.room.clone();
  }
  tokio::spawn(handle_connection(game_server, room, ws));
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
    let finished = game_server.simulate(elapsed.as_millis() as f64);
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

async fn handle_connection(game_server: Arc<Mutex<GameServer>>, room: Room, mut socket: WebSocket) {
  // Insert the write part of this peer to the peer map.
  let (tx, rx) = unbounded();

  {
    let mut game_server_unlocked = game_server.lock().unwrap();
    if game_server_unlocked.state.status == GameStatus::Connecting {
      game_server_unlocked.state.status = GameStatus::Lobby;
      tokio::spawn(simulation_loop(game_server.clone(), room.clone()));
    }
  }

  let uuid: UUID;
  println!("Waiting on initial message...");
  let message = match socket.next().await {
    None => return, // client hung up immediately
    Some(Ok(m)) => m,
    Some(Err(e)) => {
      println!("Error reading initial message from client: {:?}", e);
      return;
    }
  };

  println!("Received initial message...");
  let join_text = match message.to_str() {
    Ok(s) => s,
    Err(_) => {
      println!(
        "Client didn't introduce themselves properly, hanging up. Bad message: {:?}",
        message
      );
      return;
    }
  };
  println!("Got initial message: {}", join_text);
  let message: ClientToServerMessage = match serde_json::from_str(&join_text) {
    Ok(m) => m,
    Err(e) => {
      println!("Unable to deserialize {:?} – {:?}", join_text, e);
      return;
    }
  };
  let decoded_join = match message {
    ClientToServerMessage::Join(join) => join,
    _ => {
      println!("Client didn't introduce themselves with a join message. Hanging up");
      return;
    }
  };
  uuid = decoded_join.uuid;
  room.lock().unwrap().insert(uuid, tx);
  {
    let mut game_server = game_server.lock().unwrap();
    match game_server.handle_message(uuid, ClientToServerMessage::Join(decoded_join)) {
      Ok(_) => (),
      Err(e) => {
        println!("Failed to handle message from {:?}: {}", uuid, e);
        return;
      }
    }
  }

  let (outgoing, incoming) = socket.split();

  let broadcast_incoming = incoming.try_for_each(|msg| {
    let message_text = match msg.to_str() {
      Ok(s) => s,
      Err(_) => return future::ok(()), // other kind of message, ignore
    };
    println!("Received a message from {}: {:?}", uuid, message_text);
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

  let receive_from_others = rx.map(Ok).forward(outgoing);

  pin_mut!(broadcast_incoming, receive_from_others);
  future::select(broadcast_incoming, receive_from_others).await;

  println!("{} disconnected", uuid);
  room.lock().unwrap().remove(&uuid);

  let mut game_server = game_server.lock().unwrap();
  if game_server.state.status.finished() {
    return; // The game is done, and the simulation loop will clean up, just return.
  }
  match game_server.disconnected(uuid) {
    Ok(()) => (),
    Err(e) => println!("Error handling disconnection: {}", e),
  }
}

fn broadcast(room: Room, msg: &Message) -> Result<(), Box<dyn Error>> {
  let peers = room.lock().unwrap();
  for (_, recp) in peers.iter() {
    recp.unbounded_send(msg.clone())?;
  }
  Ok(())
}
