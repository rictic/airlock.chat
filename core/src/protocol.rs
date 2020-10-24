use crate::*;
use core::fmt::Debug;
use instant::Duration;
use serde::{Deserialize, Serialize};

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
  ReportBody {
    dead_body_color: Color,
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
      ClientToServerMessage::ReportBody { .. } => "ReportBody",
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
  Welcome { connection_id: UUID },
  Snapshot(Snapshot),
  Replay(RecordedGame),
  DisplayMessage(DisplayMessage),
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
      ServerToClientMessage::DisplayMessage(_) => "DisplayMessage",
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PlayerStartInfo {
  pub team: Team,
  pub tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DisplayMessage {
  pub message: String,
  pub duration: Duration,
  pub delay_before_show: Duration,
}
impl DisplayMessage {
  pub fn pass_time(&mut self, elapsed: Duration) {
    if self.delay_before_show != Duration::from_secs(0) {
      self.delay_before_show = self
        .delay_before_show
        .checked_sub(elapsed)
        .unwrap_or_else(|| Duration::from_secs(0));
      return;
    }
    self.duration = self
      .duration
      .checked_sub(elapsed)
      .unwrap_or_else(|| Duration::from_secs(0));
  }
  pub fn is_expired(&self) -> bool {
    self.duration == Duration::from_secs(0)
  }
  pub fn ready_to_display(&self) -> bool {
    self.delay_before_show == Duration::from_secs(0)
  }
}

impl Default for PlayerStartInfo {
  fn default() -> Self {
    Self {
      team: Team::Crew,
      tasks: (0..6).map(|_| Task::default()).collect(),
    }
  }
}
