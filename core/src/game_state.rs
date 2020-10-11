use crate::*;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;

// The full game state
#[derive(PartialEq, Clone, Debug)]
pub struct GameState {
  pub status: GameStatus,
  pub speed: f64,
  pub kill_distance: f64,
  pub task_distance: f64,
  pub players: BTreeMap<UUID, Player>,
  pub bodies: Vec<DeadBody>,
}

impl GameState {
  pub fn new(players: BTreeMap<UUID, Player>) -> GameState {
    GameState {
      status: GameStatus::Connecting,
      speed: 2.0,
      task_distance: 32.0,
      kill_distance: 64.0,
      players,
      bodies: Vec::new(),
    }
  }

  pub fn simulate(&mut self, elapsed: f64) -> bool {
    // elapsed is the time, in milliseconds, that has passed since the
    // last time we simulated.
    // By making our simulations relative to the amount of time that's passed,
    // the game will progress the same regardless of the frame rate, which may
    // vary between 30fps and 144fps even if our performance is perfect!
    let time_steps_passed = elapsed / 16.0;

    for (_, player) in self.players.iter_mut() {
      let Speed { dx, dy } = player.speed;
      player.position.x += dx * time_steps_passed;
      player.position.y += dy * time_steps_passed;
      // We don't handle inputs.q here because player position may be
      // out of sync, but we _super_ don't want to let life or death
      // get out of sync.
    }

    self.status.finished()
  }

  fn win(&mut self, team: Team) -> Result<(), String> {
    self.status = GameStatus::Won(team);
    Ok(())
  }

  pub fn note_game_started(&mut self) -> Result<(), String> {
    if self.status != GameStatus::Lobby {
      return Err(format!(
        "Internal error: got a message to start a game when not in the lobby!? Game status: {:?}",
        self.status
      ));
    }
    self.status = GameStatus::Playing;
    let impostor_index = rand::thread_rng().gen_range(0, self.players.len());
    for (i, (_, player)) in self.players.iter_mut().enumerate() {
      if i == impostor_index {
        player.impostor = true;
      }
      player.tasks = (0..6)
        .map(|_| Task {
          finished: false,
          position: Position::random(),
        })
        .collect();
    }

    Ok(())
  }

  pub fn note_death(&mut self, body: DeadBody) -> Result<(), String> {
    for (_, player) in self.players.iter_mut() {
      if player.color == body.color {
        player.dead = true;
      }
    }
    self.bodies.push(body);
    if self.impostors_outnumber_players() {
      self.win(Team::Impostors)?;
    }
    Ok(())
  }

  fn impostors_outnumber_players(&self) -> bool {
    let mut impostor_count = 0;
    let mut crew_count = 0;
    for (_, player) in self.players.iter() {
      if player.dead {
        continue;
      }
      if player.impostor {
        impostor_count += 1;
      } else {
        crew_count += 1;
      }
    }
    impostor_count >= crew_count
  }

  pub fn note_finished_task(
    &mut self,
    player_uuid: UUID,
    finished: FinishedTask,
  ) -> Result<(), String> {
    if let Some(player) = self.players.get_mut(&player_uuid) {
      if let Some(task) = player.tasks.get_mut(finished.index) {
        task.finished = true;
      }
    }
    let all_crew_tasks_finished = self
      .players
      .iter()
      .filter(|(_, p)| !p.impostor)
      .all(|(_, p)| p.tasks.iter().all(|t| t.finished));
    if all_crew_tasks_finished {
      self.win(Team::Crew)?;
    }
    Ok(())
  }
}

// This is not the proper way to handle dimensions...
pub const WIDTH: f64 = 1024.0;
pub const HEIGHT: f64 = 768.0;

// We don't use a real UUID impl because getting randomness in the browser
// is different than the server, and I got a compiler error about it.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UUID {
  v: [u8; 16],
}

// Display UUIDs as hex
impl Display for UUID {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    for byte in self.v.iter() {
      write!(fmt, "{:x?}", byte)?;
    }
    Ok(())
  }
}

impl Debug for UUID {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    for byte in self.v.iter() {
      write!(fmt, "{:x?}", byte)?;
    }
    Ok(())
  }
}

impl UUID {
  pub fn random() -> UUID {
    UUID { v: rand::random() }
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Position {
  pub x: f64,
  pub y: f64,
}

impl Position {
  pub fn distance(self, other: Position) -> f64 {
    ((self.x - other.x).powi(2) + (self.y - other.y).powi(2))
      .sqrt()
      .abs()
  }

  fn random() -> Position {
    let mut rng = rand::thread_rng();
    Position {
      x: rng.gen_range(30.0, WIDTH - 30.0),
      y: rng.gen_range(30.0, HEIGHT - 30.0),
    }
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Speed {
  pub dx: f64,
  pub dy: f64,
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
  Red,
  Pink,
  Blue,
  Orange,
  White,
  Black,
  Green,
}

impl Color {
  pub fn all() -> &'static [Color] {
    &[
      Color::Red,
      Color::Pink,
      Color::Blue,
      Color::Orange,
      Color::White,
      Color::Black,
      Color::Green,
    ]
  }

  pub fn to_str(&self) -> &'static str {
    match self {
      Color::Red => "#ff0102",
      Color::Pink => "#ff69b4",
      Color::Blue => "#1601ff",
      Color::Orange => "#ffa502",
      Color::White => "#ffffff",
      Color::Black => "#000000",
      Color::Green => "#01ff02",
    }
  }

  pub fn random() -> Color {
    Color::all()[rand::thread_rng().gen_range(0, Color::all().len())]
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Task {
  pub position: Position,
  pub finished: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Player {
  pub uuid: UUID,
  pub color: Color,
  pub position: Position,
  pub dead: bool,
  pub impostor: bool,
  pub tasks: Vec<Task>,
  pub speed: Speed,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct DeadBody {
  pub color: Color,
  pub position: Position,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum GameStatus {
  Connecting,
  Lobby,
  Playing,
  Won(Team),
  Disconnected,
}

impl GameStatus {
  pub fn finished(self) -> bool {
    match self {
      GameStatus::Connecting => false,
      GameStatus::Lobby => false,
      GameStatus::Playing => false,
      GameStatus::Won(_) => true,
      GameStatus::Disconnected => true,
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum Team {
  Crew,
  Impostors,
}
