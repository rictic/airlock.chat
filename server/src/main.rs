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

    let ws_stream = match tokio_tungstenite::accept_async(raw_stream).await {
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
        let peers = peer_map.lock().unwrap();

        // We want to broadcast the message to everyone except ourselves.
        let broadcast_recipients = peers.iter().filter(|(peer_addr, _)| peer_addr != &&addr);

        for (recp_addr, recp) in broadcast_recipients {
            if let Err(e) = recp.unbounded_send(msg.clone()) {
                println!("Error sending from {} to {}: {}", addr, recp_addr, e);
            }
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{AbortHandle, Abortable, Aborted};
    use futures_util::SinkExt;
    use std::error::Error;
    use tokio_tungstenite::connect_async;

    #[tokio::test]
    async fn test_add() -> Result<(), Box<dyn Error>> {
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let addr = "127.0.0.1:5678".to_string();
        let try_socket = TcpListener::bind(&addr).await;
        let mut listener = try_socket.expect("Failed to bind");
        let future = Abortable::new(
            tokio::spawn(async move {
                let state = PeerMap::new(Mutex::new(HashMap::new()));
                while let Ok((stream, addr)) = listener.accept().await {
                    tokio::spawn(handle_connection(state.clone(), stream, addr));
                }
            }),
            abort_registration,
        );

        let (mut client1, _) = connect_async("ws://127.0.0.1:5678/").await?;
        let (mut client2, _) = connect_async("ws://127.0.0.1:5678/").await?;
        let (mut client3, _) = connect_async("ws://127.0.0.1:5678/").await?;
        client1.send(Message::Text("abc".to_string())).await?;
        client2.send(Message::Text("def".to_string())).await?;
        assert_eq!(
            client2.next().await.ok_or("No error")??,
            Message::Text("abc".to_string())
        );
        assert_eq!(
            client3.next().await.ok_or("No error")??,
            Message::Text("abc".to_string())
        );
        assert_eq!(
            client3.next().await.ok_or("No error")??,
            Message::Text("def".to_string())
        );
        assert_eq!(
            client1.next().await.ok_or("No error")??,
            Message::Text("def".to_string())
        );
        client1.close(None).await?;
        client2.send(Message::Text("xyz".to_string())).await?;
        assert_eq!(
            client3.next().await.ok_or("No error")??,
            Message::Text("xyz".to_string())
        );

        abort_handle.abort();
        match future.await {
            Ok(_) => panic!("Aborted server exited successfully"),
            Err(a) => assert_eq!(a, Aborted),
        }

        Ok(())
    }
}
