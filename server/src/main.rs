use std::error::Error;
use std::fmt::Display;
use std::time::Duration;
use std::time::Instant;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::time::delay_for;

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

use rust_us_core::*;

type Tx = UnboundedSender<Message>;
type Room = Arc<Mutex<HashMap<UUID, Tx>>>;

struct Server {
    listener: TcpListener,
    room: Room,
    game_server: Arc<Mutex<GameServer>>,
}

struct BroadCastServer {
    room: Room,
}

impl Broadcaster for BroadCastServer {
    fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
        println!("Broadcasting {:?}", message);
        broadcast(
            self.room.clone(),
            &Message::Text(serde_json::to_string(message)?),
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
        player_connection.unbounded_send(Message::Text(serde_json::to_string(message)?))?;
        Ok(())
    }
}

impl Server {
    async fn new<A: tokio::net::ToSocketAddrs + Display>(
        addr: A,
    ) -> Result<Server, Box<dyn Error>> {
        let listener = TcpListener::bind(&addr).await?;
        println!("Listening on: {}", addr);
        let room = Room::default();
        let broadcast_server = BroadCastServer { room: room.clone() };
        Ok(Server {
            listener,
            room,
            game_server: Arc::new(Mutex::new(GameServer::new(Box::new(broadcast_server)))),
        })
    }

    async fn serve(&mut self) {
        // Let's spawn the handling of each connection in a separate task.
        while let Ok((stream, addr)) = self.listener.accept().await {
            let prev_game_finished;
            {
                let game_server = self.game_server.lock().unwrap();
                prev_game_finished = game_server.game.status.finished();
            }
            if prev_game_finished {
                // The previous game is finished. Create a new game and direct future players to it.
                let room = Room::default();
                let broadcast_server = BroadCastServer { room: room.clone() };
                self.room = room;
                self.game_server =
                    Arc::new(Mutex::new(GameServer::new(Box::new(broadcast_server))));
                println!("Starting a new game for the new client!");
            }
            tokio::spawn(handle_connection(
                self.game_server.clone(),
                self.room.clone(),
                stream,
                addr,
            ));
        }
    }
}

async fn simulation_loop(game_server: Arc<Mutex<GameServer>>) {
    let mut prev = Instant::now();
    loop {
        delay_for(Duration::from_millis(16)).await;
        let now = Instant::now();
        let elapsed = now - prev;
        prev = now;
        let mut game_server = game_server.lock().unwrap();
        game_server.simulate(elapsed.as_millis() as f64);
        if game_server.game.status.finished() {
            println!("Game finished, done simulating it on the server.");
            break;
        }
    }
}

async fn handle_connection(
    game_server: Arc<Mutex<GameServer>>,
    room: Room,
    raw_stream: TcpStream,
    addr: SocketAddr,
) {
    let mut ws_stream = match tokio_tungstenite::accept_async(raw_stream).await {
        Ok(val) => val,
        Err(e) => {
            println!("Error during the websocket handshake: {}", e);
            return;
        }
    };
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();

    {
        let mut game_server_unlocked = game_server.lock().unwrap();
        if game_server_unlocked.game.status == GameStatus::Connecting {
            game_server_unlocked.game.status = GameStatus::Lobby;
            tokio::spawn(simulation_loop(game_server.clone()));
        }
    }

    let uuid: UUID;
    println!("Waiting on initial message...");
    let message = ws_stream.next().await;
    println!("Received initial message...");
    if let Some(Ok(Message::Text(join_text))) = message {
        println!("Got initial message: {}", join_text);
        let message: ClientToServerMessage = match serde_json::from_str(&join_text) {
            Ok(m) => m,
            Err(e) => {
                println!("Unable to deserialize {:?} – {:?}", join_text, e);
                return;
            }
        };
        if let ClientToServerMessage::Join(player) = message {
            uuid = player.uuid;
            room.lock().unwrap().insert(uuid, tx);
            let mut game_server = game_server.lock().unwrap();
            match game_server.handle_message(uuid, ClientToServerMessage::Join(player)) {
                Ok(_) => (),
                Err(e) => {
                    println!("Failed to handle message from {:?}: {}", uuid, e);
                    return;
                }
            }
        } else {
            return;
        }
    } else {
        println!(
            "Client didn't introduce themselves properly, hanging up. Bad message: {:?}",
            message
        );
        // Client didn't introduce themselves properly, hang up.
        return;
    }

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        let message_text = match &msg {
            // Ignore these
            Message::Close(_) => return future::ok(()),
            Message::Ping(_) => return future::ok(()),
            Message::Pong(_) => return future::ok(()),
            Message::Binary(_) => return future::ok(()),
            // read this one
            Message::Text(t) => t,
        };
        println!(
            "Received a message from {}: {:?}",
            addr,
            msg.to_text().unwrap()
        );
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

    println!("{} disconnected", &addr);
    room.lock().unwrap().remove(&uuid);

    let mut game_server = game_server.lock().unwrap();
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:3012".to_string();
    let mut server = Server::new(addr.clone()).await?;
    println!("Listening on: {}", addr);
    server.serve().await;

    Ok(())
}
