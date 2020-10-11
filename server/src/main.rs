use std::{
    collections::HashMap,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

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
    peer_map.lock().unwrap().insert(addr, tx);

    let uuid: rust_us_core::UUID;
    if let Some(Ok(Message::Text(join_text))) = ws_stream.next().await {
        let message: rust_us_core::Message = match serde_json::from_str(&join_text) {
            Ok(m) => m,
            Err(e) => {
                println!("Unable to deserialize {:?} – {:?}", join_text, e);
                return;
            }
        };
        if let rust_us_core::Message::Join(player) = message {
            uuid = player.uuid;
        } else {
            return;
        }
        rebroadcast(peer_map.clone(), &Message::Text(join_text), addr);
    } else {
        // Client didn't introduce themselves properly, hang up.
        return;
    }

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        match &msg {
            // Ignore these
            Message::Close(_) => return future::ok(()),
            Message::Ping(_) => return future::ok(()),
            Message::Pong(_) => return future::ok(()),
            // Forward these
            Message::Text(_) => (),
            Message::Binary(_) => (),
        }
        println!(
            "Received a message from {}: {:?}",
            addr,
            msg.to_text().unwrap()
        );
        rebroadcast(peer_map.clone(), &msg, addr);

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);

    if let Ok(encoded_disconnect_message) = serde_json::to_string(
        &rust_us_core::Message::Disconnected(rust_us_core::Disconnected { uuid }),
    ) {
        rebroadcast(
            peer_map.clone(),
            &Message::Text(encoded_disconnect_message),
            addr,
        );
    }
}

fn rebroadcast(peer_map: PeerMap, msg: &Message, sender: SocketAddr) {
    let peers = peer_map.lock().unwrap();

    // We want to broadcast the message to everyone except ourselves.
    let broadcast_recipients = peers.iter().filter(|(peer_addr, _)| peer_addr != &&sender);

    for (recp_addr, recp) in broadcast_recipients {
        if let Err(e) = recp.unbounded_send(msg.clone()) {
            println!("Error sending from {} to {}: {}", sender, recp_addr, e);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let addr = "127.0.0.1:3012".to_string();
    let try_socket = TcpListener::bind(&addr).await;
    let mut listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
