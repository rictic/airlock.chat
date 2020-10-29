use crate::replay::MaybeDecisionIfPlayingBackRecording::*;
use crate::replay::{RecordingEntry, RecordingEvent};
use crate::*;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::time::Duration;

use instant::Instant;

pub trait Broadcaster: Send {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>>;
  fn send_to_player(
    &self,
    uuid: &UUID,
    message: &ServerToClientMessage,
  ) -> Result<(), Box<dyn Error>>;
}

// Implements logic for a game server without knowing about the transport layer.
// Useful so that we can implement a real game server with web sockets, and the test
// game server, and potentially a future peer to peer in-client server.
pub struct GameServer {
  pub version: String,
  pub state: GameState,
  start_time: Instant,
  last_message_received_at: Instant,
  broadcaster: Box<dyn Broadcaster>,
  recording: Option<Vec<RecordingEntry>>,
}

impl GameServer {
  pub fn new(broadcaster: Box<dyn Broadcaster>, record_game: bool) -> Self {
    let now = Instant::now();
    Self {
      version: get_version_sha().to_string(),
      state: GameState::new(),
      start_time: now,
      last_message_received_at: now,
      broadcaster,
      recording: if record_game { Some(Vec::new()) } else { None },
    }
  }

  pub fn simulate(&mut self, elapsed: Duration) -> Result<bool, Box<dyn Error>> {
    let timeout_duration = std::time::Duration::from_secs(15 * /* minutes */60);
    let timed_out = self.last_message_received_at.elapsed() > timeout_duration;
    if self.state.status != GameStatus::Connecting && timed_out {
      self.state.status = GameStatus::Disconnected;
    }
    let finished = self.state.simulate(elapsed);
    if let GameStatus::Won(_) = self.state.status {
      console_log!("Game won, trying to transmit save game");
      if let Some(recording) = &self.recording {
        console_log!("Recording exists, transmitting...");
        let replay = &ServerToClientMessage::Replay(RecordedGame::new(recording.to_vec()));
        match self.broadcaster.broadcast(replay) {
          Ok(()) => console_log!("Transmit successful!"),
          Err(e) => console_log!("Error broadcasting replay: {}", e),
        }
      }
    }
    if let GameStatus::Won(team) = self.state.status {
      self
        .broadcaster
        .broadcast(&&ServerToClientMessage::DisplayMessage(DisplayMessage {
          message: Message::PlainString(format!("{:?} win!", team)),
          duration: Duration::from_secs(15),
          delay_before_show: Duration::from_secs(0),
        }))?;
    }
    Ok(finished)
  }

  pub fn disconnected(&mut self, disconnected_player: UUID) -> Result<(), Box<dyn Error>> {
    self.record_event(&RecordingEvent::Disconnect(disconnected_player));
    if let Some(player) = self.state.players.get(&disconnected_player) {
      self
        .broadcaster
        .broadcast(&&ServerToClientMessage::DisplayMessage(DisplayMessage {
          message: Message::FormattingString(vec![
            FormattedText {
              color: Some(player.color),
              text: player.name.clone(),
            },
            FormattedText {
              color: None,
              text: " disconnected".to_string(),
            },
          ]),
          duration: Duration::from_secs(10),
          delay_before_show: Duration::from_secs(0),
        }))?;
    }
    self.state.handle_disconnection(disconnected_player);
    self.broadcast_snapshot()?;
    Ok(())
  }

  pub fn handle_message(
    &mut self,
    sender: UUID,
    message: ClientToServerMessage,
  ) -> Result<(), Box<dyn Error>> {
    let decision = self.handle_message_internal(sender, &message, &LiveGame)?;
    self.record_event(&RecordingEvent::Message(PlaybackMessage {
      sender,
      message,
      decision,
    }));
    Ok(())
  }

  pub fn handle_message_playback(
    &mut self,
    message: &PlaybackMessage,
  ) -> Result<(), Box<dyn Error>> {
    self.handle_message_internal(
      message.sender,
      &message.message,
      &Playback(message.decision.clone()),
    )?;
    Ok(())
  }

  fn handle_message_internal(
    &mut self,
    sender: UUID,
    message: &ClientToServerMessage,
    prerecorded_decision: &MaybeDecisionIfPlayingBackRecording,
  ) -> Result<Option<ServerDecision>, Box<dyn Error>> {
    self.last_message_received_at = Instant::now();
    console_log!("Game server handling {:?}", message);
    match message {
      ClientToServerMessage::StartGame() => {
        if self.state.status != GameStatus::Lobby {
          console_log!(
            "Player {} tried to start a game from state {:?}",
            sender,
            self.state.status
          );
          return Ok(None);
        }
        let start_info = match prerecorded_decision {
          LiveGame => self.state.get_game_start_info(),
          Playback(Some(ServerDecision::StartInfo(start_info))) => start_info.clone(),
          invalid => return Err(format!("Expected StartInfo when handling a recorded ClientToServerMessage::StartGame message, but got: {:?}", invalid).into()),
        };
        self.state.note_game_started(&start_info)?;
        self.broadcast_snapshot()?;
        for (uuid, player) in self.state.players.iter() {
          self.broadcaster.send_to_player(
            uuid,
            &ServerToClientMessage::DisplayMessage(DisplayMessage {
              message: Message::PlainString("The game has begun!".to_string()),
              duration: Duration::from_secs(10),
              delay_before_show: Duration::from_secs(0),
            }),
          )?;
          if player.impostor {
            self.broadcaster.send_to_player(
              uuid,
              &ServerToClientMessage::DisplayMessage(DisplayMessage {
                message: Message::PlainString(
                  "You are an evil impostor. Try to find crewmates alone and kill them! (press Q)"
                    .to_string(),
                ),
                duration: Duration::from_secs(10),
                delay_before_show: Duration::from_secs(3),
              }),
            )?;
            self.broadcaster.send_to_player(
              uuid,
              &ServerToClientMessage::DisplayMessage(DisplayMessage {
                message: Message::PlainString(
                  "But don't get caught! The crew can vote you out!".to_string(),
                ),
                duration: Duration::from_secs(10),
                delay_before_show: Duration::from_secs(6),
              }),
            )?;
          } else {
            self.broadcaster.send_to_player(
              uuid,
              &ServerToClientMessage::DisplayMessage(DisplayMessage {
                message: Message::PlainString(
                  "You are good crewmate. Find your tasks and complete them (press E)".to_string(),
                ),
                duration: Duration::from_secs(10),
                delay_before_show: Duration::from_secs(3),
              }),
            )?;
            let num_impostors = self
              .state
              .players
              .iter()
              .filter(|(_, p)| p.impostor)
              .count();
            self.broadcaster.send_to_player(
              uuid,
              &ServerToClientMessage::DisplayMessage(DisplayMessage {
                message: Message::PlainString(format!("But beware, there's {} evil impostor{} on board. If you find a dead body, press R to report it", num_impostors, if num_impostors == 1 {""} else {"s"})),
                duration: Duration::from_secs(10),
                delay_before_show: Duration::from_secs(6),
              }),
            )?;
          }
        }
        return Ok(Some(ServerDecision::StartInfo(start_info)));
      }
      ClientToServerMessage::Killed(body) => {
        self.state.note_death(*body)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::ReportBody { dead_body_color } => {
        match self.state.status {
          GameStatus::Playing(PlayState::Night) => {
            // This is fine.
          }
          _ => {
            // Invalid time to report a body, ignore it
            return Ok(None);
          }
        }
        let body = match self
          .state
          .bodies
          .iter()
          .find(|b| b.color == *dead_body_color)
        {
          None => {
            // No such body
            return Ok(None);
          }
          Some(body) => body,
        };
        let reporter = match self.state.players.get(&sender) {
          None => {
            // Reporter is a spectator lol
            return Ok(None);
          }
          Some(player) => {
            if player.dead {
              // ... but nobody came
              return Ok(None);
            }
            player
          }
        };
        let distance_between_reporter_and_body = reporter.position.distance(&body.position);
        let slop_for_latency = 32.0;
        if distance_between_reporter_and_body
          > self.state.settings.report_distance + slop_for_latency
        {
          // body is too far away to report it
          return Ok(None);
        }
        // oh shit it's on
        self.state.status = GameStatus::Playing(PlayState::Day(DayState {
          votes: BTreeMap::new(),
          time_remaining: self.state.settings.voting_time,
        }));
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::FinishedTask(finished) => {
        self.state.note_finished_task(sender, *finished)?;
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Move(moved) => {
        if let Some(player) = self.state.players.get_mut(&sender) {
          player.velocity = moved.speed;
          player.position = moved.position;
        }
        self.broadcast_snapshot()?;
      }
      ClientToServerMessage::Join {
        version,
        details: join,
      } => {
        if version != &self.version {
          // TODO: send an error and close the connection.
          return Err(
            format!(
              "Bad version in client join, need {} but got {}",
              self.version, version
            )
            .into(),
          );
        }
        let mut decision = None;
        if self.state.status == GameStatus::Lobby {
          if let JoinRequest::JoinAsPlayer {
            name,
            preferred_color,
          } = join
          {
            if self.state.players.get(&sender).is_some() {
              return Ok(None); // we know about this player already
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
                  color = &*c;
                }
              }
            } else {
              // player's preferred color wasn't taken, they're good to go!
              add_player = true;
            }
            if add_player {
              let position = match prerecorded_decision {
                LiveGame => self.state.map.get_spawn_in_position(),
                Playback(Some(ServerDecision::NewPlayerPosition(pos))) => *pos,
                _ => {
                  return Err(
                    format!(
                      "Internal error: bad recording. Expected NewPlayerPosition, got {:?}",
                      prerecorded_decision
                    )
                    .into(),
                  )
                }
              };
              decision = Some(ServerDecision::NewPlayerPosition(position));
              // Add the new player (possibly with a new color)
              let player = Player::new(sender, name.to_string(), *color, position);
              self.state.players.insert(sender, player);
              self
                .broadcaster
                .broadcast(&ServerToClientMessage::DisplayMessage(DisplayMessage {
                  message: Message::FormattingString(vec![
                    FormattedText {
                      color: Some(*color),
                      text: name.clone(),
                    },
                    FormattedText {
                      color: None,
                      text: " has_joined".into(),
                    },
                  ]),
                  duration: Duration::from_secs(10),
                  delay_before_show: Duration::from_secs(0),
                }))?;
            }
          }
          // In all other cases, they're joining as a spectator.
        }

        console_log!("Player joined? Sending welcome to player");
        self.broadcaster.send_to_player(
          &sender,
          &ServerToClientMessage::Welcome {
            connection_id: sender,
          },
        )?;
        self.broadcaster.send_to_player(
          &sender,
          &&ServerToClientMessage::DisplayMessage(DisplayMessage {
            message: Message::PlainString("Welcome to airlock.chat!".to_string()),
            duration: Duration::from_secs(10),
            delay_before_show: Duration::from_secs(0),
          }),
        )?;

        // Send out a snapshot to catch the new client up, whether or not they're playing.
        self.broadcast_snapshot()?;
        return Ok(decision);
      }
      ClientToServerMessage::Vote { target } => {
        if !(eligable_to_vote(self.state.players.get(&sender)) && self.eligable_target(*target)) {
          return Ok(None);
        }
        // If it's day, and the sender hasn't voted yet, record their vote.
        if let GameStatus::Playing(PlayState::Day(DayState { votes, .. })) = &mut self.state.status
        {
          if let Entry::Vacant(o) = votes.entry(sender) {
            o.insert(*target);
          }
        }
        self.broadcast_snapshot()?;
      }
    };
    Ok(None)
  }

  pub fn get_uuid_for_new_connection(&self) -> UUID {
    UUID::random()
  }

  pub fn broadcast_snapshot(&self) -> Result<(), Box<dyn Error>> {
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

  fn record_event(&mut self, event: &RecordingEvent) {
    let recording = match &mut self.recording {
      None => return,
      Some(r) => r,
    };
    recording.push(RecordingEntry {
      since_start: self.start_time.elapsed(),
      event: event.clone(),
    });
  }
}

fn eligable_to_vote(voter: Option<&Player>) -> bool {
  match voter {
    Some(player) => player.eligable_to_vote(),
    None => false,
  }
}
