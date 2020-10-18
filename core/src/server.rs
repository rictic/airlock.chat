use crate::*;
use std::collections::btree_map::Entry;
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
    self.state.handle_disconnection(disconnected_player);
    self.broadcast_snapshot()?;
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
      ClientToServerMessage::Join(join) => {
        if self.state.status == GameStatus::Lobby {
          if let JoinRequest::JoinAsPlayer {
            name,
            preferred_color,
          } = join
          {
            if self.state.players.get(&sender).is_some() {
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
                .insert(sender, Player::new(sender, name, color));
            }
          }
          // In all other cases, they're joining as a spectator.
        }

        self.broadcaster.send_to_player(
          &sender,
          &ServerToClientMessage::Welcome {
            connection_id: sender,
          },
        )?;
        // Send out a snapshot to catch the new client up, whether or not they're playing.
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Vote { target } => {
        if !(eligable_to_vote(self.state.players.get(&sender)) && self.eligable_target(target)) {
          return Ok(());
        }
        // If it's day, and the sender hasn't voted yet, record their vote.
        if let GameStatus::Playing(PlayState::Day(DayState { votes, .. })) = &mut self.state.status
        {
          if let Entry::Vacant(o) = votes.entry(sender) {
            o.insert(target);
          }
        }
      }
    };
    Ok(())
  }

  pub fn get_uuid_for_new_connection(&self) -> UUID {
    UUID::random()
  }

  fn broadcast_snapshot(&self) -> Result<(), Box<dyn Error>> {
    self
      .broadcaster
      .broadcast(&ServerToClientMessage::Snapshot(Snapshot {
        status: self.state.status.clone(),
        bodies: self.state.bodies.clone(),
        players: self.state.players.iter().map(|(_, p)| p.clone()).collect(),
      }))?;
    Ok(())
  }

  fn eligable_target(&self, target: VoteTarget) -> bool {
    match target {
      VoteTarget::Skip => {
        true // skip is always valid
      }
      VoteTarget::Player { uuid } => match self.state.players.get(&uuid) {
        None => false,
        Some(p) => !p.dead,
      },
    }
  }
}

fn eligable_to_vote(voter: Option<&Player>) -> bool {
  match voter {
    Some(player) => player.eligable_to_vote(),
    None => false,
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
