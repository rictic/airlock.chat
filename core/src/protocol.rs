use crate::*;
use core::fmt::Debug;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
  Move(MoveMessage),
  Killed(DeadBody),
  FinishedTask(FinishedTask),
  Join(Join),
  Vote { target: VoteTarget },
  StartGame(),
}

impl ClientToServerMessage {
  #[allow(dead_code)]
  pub fn kind(&self) -> &'static str {
    match self {
      ClientToServerMessage::Move(_) => "Move",
      ClientToServerMessage::Killed(_) => "Killed",
      ClientToServerMessage::FinishedTask(_) => "FinishedTask",
      ClientToServerMessage::Join(_) => "Join",
      ClientToServerMessage::StartGame() => "StartGame",
      ClientToServerMessage::Vote { .. } => "Vote",
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
  Snapshot(Snapshot),
}

impl ServerToClientMessage {
  #[allow(dead_code)]
  pub fn kind(&self) -> &'static str {
    match self {
      ServerToClientMessage::Snapshot(_) => "Snapshot",
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Join {
  pub uuid: UUID,
  pub preferred_color: Color,
  pub name: String,
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
