#![cfg(test)]
use crate::*;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

#[test]
fn pythagoras_was_right() {
  assert!((5.0 - (Position { x: 3.0, y: 0.0 }).distance(Position { x: 0.0, y: 4.0 })).abs() < 0.01);
}

struct TestEnvironment {
  game_server: GameServer,
  server_to_client_queue: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>>,
  players: HashMap<UUID, GameAsPlayer>,
  player_queue: HashMap<UUID, Arc<Mutex<Vec<ClientToServerMessage>>>>,
}
impl TestEnvironment {
  fn new() -> TestEnvironment {
    let messages: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>> = Arc::default();
    TestEnvironment {
      game_server: GameServer::new(Box::new(TestBroadcaster {
        players: messages.clone(),
      })),
      server_to_client_queue: messages,
      players: HashMap::default(),
      player_queue: HashMap::default(),
    }
  }

  // This creates a player, and establishes a bidirectional connection in our
  // fake network, but doesn't send any messages.
  fn create_player(&mut self) -> UUID {
    let queue: Arc<Mutex<Vec<ClientToServerMessage>>> = Arc::default();
    let player = GameAsPlayer::new(Box::new(TestPlayerTx {
      messages: queue.clone(),
    }));
    let uuid = player.my_uuid;
    self
      .server_to_client_queue
      .lock()
      .unwrap()
      .insert(uuid, vec![]);
    self.players.insert(uuid, player);
    self.player_queue.insert(uuid, queue);
    if self.game_server.game.status == GameStatus::Connecting {
      self.game_server.game.status = GameStatus::Lobby;
    }
    uuid
  }

  // This removes a player from the fake network. Because a server would
  // also notice the disconnection separate from any message, it also
  // notifies the server of the disconnection.
  fn remove_player(&mut self, uuid: UUID) -> Result<GameAsPlayer, Box<dyn Error>> {
    self
      .server_to_client_queue
      .lock()
      .unwrap()
      .remove(&uuid)
      .ok_or("Not in server_to_client_queue")?;
    self
      .player_queue
      .remove(&uuid)
      .ok_or("not in player_queue")?;
    let result = self.players.remove(&uuid).ok_or("not in players")?;
    self.game_server.disconnected(uuid)?;
    Ok(result)
  }

  // Pass messages from server to client and client to server until we
  // reach a steady state where no one has any more messages for anyone else.
  fn dispatch_messages(&mut self) -> Result<(), Box<dyn Error>> {
    let mut keep_going = true;
    while keep_going {
      keep_going = false;
      for (uuid, messages_lock) in self.player_queue.iter_mut() {
        let messages: Vec<ClientToServerMessage>;
        {
          let mut messages_local = messages_lock.lock().unwrap();
          if messages_local.is_empty() {
            continue;
          }
          keep_going = true;
          messages = messages_local.clone();
          messages_local.clear();
        }
        for message in messages {
          println!("Passing {} to server from {}", message.kind(), uuid);
          self.game_server.handle_message(*uuid, message)?;
        }
      }
      let mut queue = self.server_to_client_queue.lock().unwrap();
      println!(
        "Processing {} players in server_to_client_queue",
        queue.len()
      );
      for (uuid, messages) in queue.iter_mut() {
        if messages.is_empty() {
          println!("Messages to {} are empty", uuid);
          continue;
        }
        keep_going = true;
        let messages_local = messages.clone();
        for message in messages_local {
          println!("Passing {} from server to {}", message.kind(), uuid);
          let player = self.players.get_mut(uuid).unwrap();
          player.handle_msg(message)?;
        }
        messages.clear();
      }
      println!(
        "Finished one loop of dispatch_messages. Keep going? {}",
        keep_going
      );
    }
    Ok(())
  }

  fn expect_everyone_agrees_on_game_state(
    &self,
    expected_num_players: usize,
  ) -> Result<Game, Box<dyn Error>> {
    let server_state = self.game_server.game.clone();
    assert_eq!(self.players.len(), expected_num_players);
    for (_, game_as_player) in self.players.iter() {
      // Will eventually need to customize this more, because players will
      // only know a subset.
      assert_eq!(&server_state, &game_as_player.game);
    }
    Ok(server_state)
  }

  fn time_passes(&mut self, elapsed: f64) {
    self.game_server.game.simulate(elapsed);
    for (_, player) in self.players.iter_mut() {
      player.game.simulate(elapsed);
    }
  }

  fn create_and_connect_player(&mut self) -> Result<UUID, Box<dyn Error>> {
    let player_count = self.players.len();
    let id = self.create_player();
    self.players.get_mut(&id).unwrap().connected()?;
    self.dispatch_messages()?;
    self.expect_everyone_agrees_on_game_state(player_count + 1)?;
    Ok(id)
  }
}

struct TestBroadcaster {
  players: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>>,
}
impl Broadcaster for TestBroadcaster {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
    println!("Broadcasting {} from server", message.kind());
    let mut players = self.players.lock().unwrap();
    for (_uuid, messages) in players.iter_mut() {
      messages.push(message.clone());
    }
    println!("Broadcast complete");
    Ok(())
  }
  fn send_to_player(
    &self,
    uuid: &UUID,
    message: &ServerToClientMessage,
  ) -> Result<(), Box<dyn Error>> {
    println!("Sending {} to player {} from server", message.kind(), uuid);
    let mut players = self.players.lock().unwrap();
    if let Some(messages) = players.get_mut(uuid) {
      messages.push(message.clone());
    }
    Ok(())
  }
}

#[derive(Default)]
struct TestPlayerTx {
  messages: Arc<Mutex<Vec<ClientToServerMessage>>>,
}
impl GameTx for TestPlayerTx {
  fn send(&self, message: &ClientToServerMessage) -> Result<(), String> {
    self.messages.lock().unwrap().push((*message).clone());
    Ok(())
  }
}

#[test]
fn test_connection_and_disconnection() -> Result<(), Box<dyn Error>> {
  let mut env = TestEnvironment::new();
  env.expect_everyone_agrees_on_game_state(0)?;

  // P1 connects
  let player1_id = env.create_player();
  let player1 = env.players.get_mut(&player1_id).unwrap();
  player1.connected()?;
  env.dispatch_messages()?;
  env.expect_everyone_agrees_on_game_state(1)?;

  // P2 connects
  let player2_id = env.create_player();
  let player2 = env.players.get_mut(&player2_id).unwrap();
  player2.connected()?;
  env.dispatch_messages()?;
  env.expect_everyone_agrees_on_game_state(2)?;

  // P3 connects
  let player3_id = env.create_player();
  let player3 = env.players.get_mut(&player3_id).unwrap();
  player3.connected()?;
  env.dispatch_messages()?;
  let game = env.expect_everyone_agrees_on_game_state(3)?;

  // P2 disconnects
  assert!(game.players.get(&player2_id).is_some());
  env.remove_player(player2_id)?;
  env.dispatch_messages()?;
  let game = env.expect_everyone_agrees_on_game_state(2)?;
  assert!(game.players.get(&player2_id).is_none());

  Ok(())
}

#[test]
fn test_movement() -> Result<(), Box<dyn Error>> {
  let mut env = TestEnvironment::new();
  env.expect_everyone_agrees_on_game_state(0)?;

  let player1_id = env.create_and_connect_player()?;
  let player2_id = env.create_and_connect_player()?;
  let player3_id = env.create_and_connect_player()?;

  // Move p1 up and left
  let p1_client = env.players.get_mut(&player1_id).unwrap();
  p1_client.take_input(InputState {
    up: true,
    left: true,
    right: false,
    down: false,
    activate: false,
    kill: false,
    play: false,
    report: false,
  })?;
  let p1_position = p1_client.local_player().unwrap().position;

  // Don't move p2.
  let p2_position = env
    .players
    .get_mut(&player2_id)
    .unwrap()
    .local_player()
    .unwrap()
    .position;

  // Move p3 down and right
  let p3_client = env.players.get_mut(&player3_id).unwrap();
  p3_client.take_input(InputState {
    up: false,
    left: false,
    right: true,
    down: true,
    activate: false,
    kill: false,
    play: false,
    report: false,
  })?;
  let p3_position = p3_client.local_player().unwrap().position;

  // Let four time ticks pass
  env.dispatch_messages()?;
  env.time_passes(64.0);

  let player_positions: HashMap<UUID, Position> = env
    .game_server
    .game
    .players
    .iter()
    .map(|(u, p)| (*u, p.position))
    .collect();

  // P1 moved up and to the left
  let updated_p1_position = player_positions.get(&player1_id).unwrap();
  assert!(updated_p1_position.x < p1_position.x);
  assert!(updated_p1_position.y < p1_position.y);
  // P2's position hasn't changed
  assert_eq!(player_positions.get(&player2_id), Some(&p2_position));
  let updated_p3_position = player_positions.get(&player3_id).unwrap();
  // P3 moved down and to the right
  assert!(updated_p3_position.x > p3_position.x);
  assert!(updated_p3_position.y > p3_position.y);

  // Clients and server agree
  env.expect_everyone_agrees_on_game_state(3)?;

  Ok(())
}
