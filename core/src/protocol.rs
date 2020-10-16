use crate::*;
use core::fmt::Debug;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
  Move(MoveMessage),
  Killed(DeadBody),
  FinishedTask(FinishedTask),
  Join {
    version: String,
    details: JoinRequest,
  },
  Vote {
    target: VoteTarget,
  },
  StartGame(),
}

impl ClientToServerMessage {
  #[allow(dead_code)]
  pub fn kind(&self) -> &'static str {
    match self {
      ClientToServerMessage::Move(_) => "Move",
      ClientToServerMessage::Killed(_) => "Killed",
      ClientToServerMessage::FinishedTask(_) => "FinishedTask",
      ClientToServerMessage::Join { .. } => "Join",
      ClientToServerMessage::StartGame() => "StartGame",
      ClientToServerMessage::Vote { .. } => "Vote",
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
  Welcome { connection_id: UUID },
  Snapshot(Snapshot),
  Replay(RecordedGame),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum JoinRequest {
  JoinAsPlayer {
    preferred_color: Color,
    name: String,
  },
  JoinAsSpectator,
}

impl ServerToClientMessage {
  #[allow(dead_code)]
  pub fn kind(&self) -> &'static str {
    match self {
      ServerToClientMessage::Welcome { .. } => "Welcome",
      ServerToClientMessage::Snapshot(_) => "Snapshot",
      ServerToClientMessage::Replay(_) => "Replay",
    }
  }
}

pub fn get_version_sha() -> &'static str {
  env!("VERGEN_SHA")
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct MoveMessage {
  pub speed: Speed,
  pub position: Position,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct FinishedTask {
  pub index: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Snapshot {
  pub status: GameStatus,
  pub bodies: Vec<DeadBody>,
  pub players: Vec<Player>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StartGame {
  pub impostors: Vec<UUID>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Disconnected {
  pub uuid: UUID,
}

#[derive(Debug, Clone)]
pub enum MaybeDecisionIfPlayingBackRecording {
  LiveGame,
  Playback(Option<ServerDecision>),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecordedGame {
  // The version of the software this was recorded with.
  version: String,
  entries: Vec<RecordingEntry>,
}
impl RecordedGame {
  pub fn new(entries: Vec<RecordingEntry>) -> Self {
    Self {
      version: get_version_sha().to_string(),
      entries,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecordingEntry {
  pub since_start: Duration,
  pub event: RecordingEvent,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RecordingEvent {
  Message(PlaybackMessage),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlaybackMessage {
  pub sender: UUID,
  pub message: ClientToServerMessage,
  pub decision: Option<ServerDecision>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerDecision {
  StartInfo(BTreeMap<UUID, PlayerStartInfo>),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerStartInfo {
  pub team: Team,
  pub tasks: Vec<Task>,
}

impl Default for PlayerStartInfo {
  fn default() -> Self {
    Self {
      team: Team::Crew,
      tasks: (0..6).map(|_| Task::default()).collect(),
    }
  }
}
