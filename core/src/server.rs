use crate::*;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;

use std::time::Instant;

// Implements logic for a game server without knowing about the transport layer.
// Useful so that we can implement a real game server with web sockets, and the test
// game server, and potentially a future peer to peer in-client server.
pub struct GameServer {
  pub game: Game,
  last_message_received_at: Instant,
  broadcaster: Box<dyn Broadcaster>,
}

impl GameServer {
  pub fn new(broadcaster: Box<dyn Broadcaster>) -> GameServer {
    GameServer {
      game: Game::new(BTreeMap::new()),
      last_message_received_at: Instant::now(),
      broadcaster,
    }
  }

  pub fn simulate(&mut self, elapsed: f64) -> bool {
    let timeout_duration = std::time::Duration::from_secs(15 * /* minutes */60);
    let timed_out = self.last_message_received_at.elapsed() > timeout_duration;
    if self.game.status != GameStatus::Connecting && timed_out {
      self.game.status = GameStatus::Disconnected;
    }
    self.game.simulate(elapsed)
  }

  pub fn disconnected(&mut self, disconnected_player: UUID) -> Result<(), Box<dyn Error>> {
    self.game.players.remove(&disconnected_player);
    self.broadcast_snapshot()?;
    if self.game.players.is_empty() {
      self.game.status = GameStatus::Disconnected;
    }
    Ok(())
  }

  pub fn handle_message(
    &mut self,
    sender: UUID,
    message: ClientToServerMessage,
  ) -> Result<(), Box<dyn Error>> {
    self.last_message_received_at = Instant::now();
    println!("Game server handling {:?}", message);
    match message {
      ClientToServerMessage::StartGame() => {
        if self.game.status != GameStatus::Lobby {
          print!(
            "Player {} tried to start a game from state {:?}",
            sender, self.game.status
          );
          return Ok(());
        }
        self.game.note_game_started()?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Killed(body) => {
        self.game.note_death(body)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::FinishedTask(finished) => {
        self.game.note_finished_task(sender, finished)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Move(moved) => {
        if let Some(player) = self.game.players.get_mut(&sender) {
          player.speed = moved.speed;
          player.position = moved.position;
        }
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Join(mut player) => {
        if self.game.status == GameStatus::Lobby {
          if self.game.players.get(&player.uuid).is_some() {
            return Ok(()); // we know about this player already
          }
          // ok, it's a new player, and we have room for them. if their color is
          // already taken, give them a new one.
          let taken_colors: BTreeSet<Color> =
            self.game.players.iter().map(|(_, p)| p.color).collect();
          let add_player;
          if taken_colors.contains(&player.color) {
            match Color::all().iter().find(|c| !taken_colors.contains(c)) {
              None => {
                add_player = false; // we can't add this player, all colors are taken!
              }
              Some(c) => {
                add_player = true;
                player.color = *c;
              }
            }
          } else {
            // player's color wasn't taken, they're good to go!
            add_player = true;
          }
          if add_player {
            // We've added the new player (possibly with a new color)
            self.game.players.insert(player.uuid, player);
          }
        }

        // Send out a snapshot to catch the new client up, whether or not they're playing.
        self.broadcast_snapshot()?;
      }
    };
    Ok(())
  }

  fn broadcast_snapshot(&self) -> Result<(), Box<dyn Error>> {
    self
      .broadcaster
      .broadcast(&ServerToClientMessage::Snapshot(Snapshot {
        status: self.game.status,
        bodies: self.game.bodies.clone(),
        players: self.game.players.iter().map(|(_, p)| p.clone()).collect(),
      }))?;
    Ok(())
  }
}

pub trait Broadcaster: Send {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>>;
  fn send_to_player(
    &self,
    uuid: &UUID,
    message: &ServerToClientMessage,
  ) -> Result<(), Box<dyn Error>>;
}
