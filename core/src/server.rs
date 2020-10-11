use crate::*;
use std::collections::BTreeSet;
use std::error::Error;

use std::time::Instant;

// Implements logic for a game server without knowing about the transport layer.
// Useful so that we can implement a real game server with web sockets, and the test
// game server, and potentially a future peer to peer in-client server.
pub struct GameServer {
  pub state: GameState,
  last_message_received_at: Instant,
  broadcaster: Box<dyn Broadcaster>,
}

impl GameServer {
  pub fn new(broadcaster: Box<dyn Broadcaster>) -> GameServer {
    GameServer {
      state: GameState::new(),
      last_message_received_at: Instant::now(),
      broadcaster,
    }
  }

  pub fn simulate(&mut self, elapsed: f64) -> bool {
    let timeout_duration = std::time::Duration::from_secs(15 * /* minutes */60);
    let timed_out = self.last_message_received_at.elapsed() > timeout_duration;
    if self.state.status != GameStatus::Connecting && timed_out {
      self.state.status = GameStatus::Disconnected;
    }
    self.state.simulate(elapsed)
  }

  pub fn disconnected(&mut self, disconnected_player: UUID) -> Result<(), Box<dyn Error>> {
    self.state.players.remove(&disconnected_player);
    self.broadcast_snapshot()?;
    if self.state.players.is_empty() {
      self.state.status = GameStatus::Disconnected;
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
        if self.state.status != GameStatus::Lobby {
          print!(
            "Player {} tried to start a game from state {:?}",
            sender, self.state.status
          );
          return Ok(());
        }
        self.state.note_game_started()?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Killed(body) => {
        self.state.note_death(body)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::FinishedTask(finished) => {
        self.state.note_finished_task(sender, finished)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Move(moved) => {
        if let Some(player) = self.state.players.get_mut(&sender) {
          player.speed = moved.speed;
          player.position = moved.position;
        }
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Join(Join {
        uuid,
        name,
        preferred_color,
      }) => {
        if self.state.status == GameStatus::Lobby {
          if self.state.players.get(&uuid).is_some() {
            return Ok(()); // we know about this player already
          }
          // ok, it's a new player, and we have room for them. if their color is
          // already taken, give them a new one.
          let taken_colors: BTreeSet<Color> =
            self.state.players.iter().map(|(_, p)| p.color).collect();
          let add_player;
          let mut color = preferred_color;
          if taken_colors.contains(&color) {
            match Color::all().iter().find(|c| !taken_colors.contains(c)) {
              None => {
                add_player = false; // we can't add this player, all colors are taken!
              }
              Some(c) => {
                add_player = true;
                color = *c;
              }
            }
          } else {
            // player's preferred color wasn't taken, they're good to go!
            add_player = true;
          }
          if add_player {
            // Add the new player (possibly with a new color)
            self
              .state
              .players
              .insert(uuid, Player::new(uuid, name, color));
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
        status: self.state.status,
        bodies: self.state.bodies.clone(),
        players: self.state.players.iter().map(|(_, p)| p.clone()).collect(),
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
