use crate::*;
use core::time::Duration;
use rand::Rng;
use serde::de::{self, Visitor};
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fmt::Display;
use std::{collections::BTreeMap, iter::FromIterator};
use std::{f64::consts::PI, fmt};

#[derive(PartialEq, Clone, Debug)]
pub struct Settings {
  pub speed: f64,
  pub kill_distance: f64,
  pub task_distance: f64,
  pub report_distance: f64,
  pub crew_vision: f64,
  pub impostor_vision: f64,
  pub voting_time: Duration,
  pub num_tasks: usize,
}

impl Default for Settings {
  fn default() -> Self {
    Settings {
      speed: 2.0,
      kill_distance: 64.0,
      task_distance: 32.0,
      report_distance: 96.0,
      crew_vision: 176.0,
      impostor_vision: 256.0,
      voting_time: Duration::from_secs(120),
      num_tasks: 6,
    }
  }
}

impl Default for GameState {
  fn default() -> Self {
    Self::new()
  }
}

// The full game state
#[derive(PartialEq, Clone, Debug)]
pub struct GameState {
  pub status: GameStatus,
  pub settings: Settings,
  pub map: Map,
  pub players: BTreeMap<UUID, Player>,
  pub bodies: Vec<DeadBody>,
}

impl GameState {
  pub fn new() -> Self {
    GameState {
      status: GameStatus::Connecting,
      settings: Settings::default(),
      players: BTreeMap::new(),
      bodies: Vec::new(),
      map: Map::first_map(),
    }
  }

  pub fn simulate(&mut self, elapsed: Duration) -> bool {
    self.status.progress_time(elapsed);
    match &self.status {
      GameStatus::Lobby | GameStatus::Playing(PlayState::Night) => self.simulate_night(elapsed),
      GameStatus::Playing(PlayState::Day(day_state)) => {
        if self.is_day_over(day_state) {
          match day_state.determine_winner_of_election() {
            VoteTarget::Skip => { /* The crew have chosen a strange mercy */ }
            VoteTarget::Player { uuid } => {
              // Kill the lucky winner!
              if let Some(player) = self.players.get_mut(&uuid) {
                player.dead = true;
              }
            }
          }
          console_log!("Day is done, now it's night!");
          self.check_for_victories();
          self.bodies.clear();
          self.map.place_players_at_night_start(&mut self.players);
          // Now it's night!
          self.status = GameStatus::Playing(PlayState::Night);
        }
      }
      GameStatus::Connecting | GameStatus::Disconnected | GameStatus::Won(_) => {
        // Nothing to simulate
      }
    }

    self.status.finished()
  }

  fn is_day_over(&self, day_state: &DayState) -> bool {
    // Day can end after a timer.
    if day_state.time_remaining <= Duration::from_secs(0) {
      return true;
    }
    // Or after all eligable players have recorded a vote.
    self
      .players
      .iter()
      .filter(|(_, p)| p.eligable_to_vote())
      .all(|(uuid, _)| day_state.votes.contains_key(uuid))
  }

  fn simulate_night(&mut self, elapsed: Duration) {
    // elapsed is the time, in milliseconds, that has passed since the
    // last time we simulated.
    // By making our simulations relative to the amount of time that's passed,
    // the game will progress the same regardless of the frame rate, which may
    // vary between 30fps and 144fps even if our performance is perfect!
    let time_steps_passed =
      (elapsed.as_nanos() as f64) / (Duration::from_millis(16).as_nanos() as f64);

    for (_, player) in self.players.iter_mut() {
      if player.velocity.dx == 0.0 && player.velocity.dy == 0.0 {
        continue;
      }

      let mut movement_vector = Velocity {
        dx: player.velocity.dx * time_steps_passed,
        dy: player.velocity.dy * time_steps_passed,
      };

      for shape in self.map.static_geometry.iter() {
        movement_vector = shape.collide(player.position, Player::radius(), movement_vector, 0.10);
      }

      // Advance the player
      let new_pos = Position {
        x: player.position.x + movement_vector.dx,
        y: player.position.y + movement_vector.dy,
      };

      // Bound their new position within within the map
      let new_pos = self
        .map
        .constrain_circle_within_bounds(new_pos, Player::radius());

      player.position = new_pos;
    }
  }

  fn win(&mut self, team: Team) {
    self.status = GameStatus::Won(team);
  }

  pub fn get_game_start_info(&self) -> StartInfo {
    let mut assignments: BTreeMap<UUID, PlayerStartInfo> = self
      .players
      .keys()
      .map(|k| (*k, PlayerStartInfo::new(&self.map, &self.settings)))
      .collect();
    let impostor_index = rand::thread_rng().gen_range(0, self.players.len());
    for (i, (_uuid, player_start_info)) in assignments.iter_mut().enumerate() {
      if i == impostor_index {
        player_start_info.team = Team::Impostors;
      }
    }
    StartInfo {
      assignments: assignments.into_iter().collect(),
    }
  }

  pub fn note_game_started(&mut self, start_info: &StartInfo) -> Result<(), String> {
    if self.status != GameStatus::Lobby {
      return Err(format!(
        "Internal error: got a message to start a game when not in the lobby!? Game status: {:?}",
        self.status
      ));
    }
    for (uuid, start_info) in start_info.assignments.iter() {
      if let Some(player) = self.players.get_mut(uuid) {
        player.impostor = start_info.team == Team::Impostors;
        player.tasks = start_info.tasks.clone();
      } else {
        return Err(format!(
          "Unable to find player with uuid {} when starting game.",
          uuid
        ));
      }
    }
    self.status = GameStatus::Playing(PlayState::Night);
    self.map.place_players_at_game_start(&mut self.players);
    Ok(())
  }

  pub fn note_death(&mut self, body: DeadBody) -> Result<(), String> {
    for (_, player) in self.players.iter_mut() {
      if player.color == body.color {
        player.dead = true;
      }
    }
    self.bodies.push(body);
    self.check_for_impostor_win();
    Ok(())
  }

  fn check_for_impostor_win(&mut self) {
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
    if impostor_count >= crew_count {
      self.win(Team::Impostors);
    }
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
    self.check_for_crew_win();
    Ok(())
  }

  fn check_for_crew_win(&mut self) {
    let all_crew_tasks_finished = self
      .players
      .iter()
      .filter(|(_, p)| !p.impostor)
      .all(|(_, p)| p.tasks.iter().all(|t| t.finished));
    if all_crew_tasks_finished {
      self.win(Team::Crew);
    }
  }

  fn check_for_victories(&mut self) {
    match self.status {
      GameStatus::Connecting
      | GameStatus::Disconnected
      | GameStatus::Lobby
      | GameStatus::Won(_) => return,
      GameStatus::Playing(_) => (),
    }
    // The game might be over because the crew has won!
    self.check_for_crew_win();
    // The game might be over because the impostors have won D:
    self.check_for_impostor_win();
  }

  pub fn handle_disconnection(&mut self, disconnected_player: UUID) {
    self.players.remove(&disconnected_player);
    // The game might be over, because we're out of players
    if self.players.is_empty() {
      self.status = GameStatus::Disconnected;
    }
    self.check_for_victories();
    // We might be voting, in which case we want to remove all votes for the
    // disconnected player, so that players can vote for someone else if they wish.
    if let GameStatus::Playing(PlayState::Day(day)) = &mut self.status {
      let mut voters_for_disonnected = Vec::new();
      for (voter, target) in day.votes.iter_mut() {
        if let VoteTarget::Player { uuid } = target {
          if *uuid == disconnected_player {
            voters_for_disonnected.push(*voter);
          }
        }
      }
      for voter in voters_for_disonnected {
        day.votes.remove(&voter);
      }
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Map {
  width: f64,
  height: f64,
  start_circle: (Position, f64),
  meeting_circle: (Position, f64),
  pub static_geometry: Vec<Shape>,
}

impl Map {
  fn first_map() -> Map {
    let conference_table = Position { x: 275.0, y: 275.0 };
    Map {
      width: 3036.0,
      height: 768.0,
      start_circle: (
        Position {
          x: 1900.0,
          y: 500.0,
        },
        100.0,
      ),
      meeting_circle: (conference_table, 100.0),
      static_geometry: vec![
        // conference table
        Shape::Circle {
          radius: 75.0,
          center: conference_table,
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#358".into(),
        },
        // nega table
        Shape::Circle {
          radius: 62.0,
          center: Position { x: 875.0, y: 505.0 },
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#a22".into(),
        },
        Shape::Circle {
          radius: 62.0,
          center: Position { x: 1675.0, y: 95.0 },
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#a22".into(),
        },
        Shape::Circle {
          radius: 140.0,
          center: Position {
            x: 2475.0,
            y: 190.0,
          },
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#666".into(),
        },
        Shape::Circle {
          radius: 140.0,
          center: Position {
            x: 2475.0,
            y: 190.0 + 140.0 + 140.0 + 30.0,
          },
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#666".into(),
        },
        Shape::Circle {
          radius: 62.0,
          center: Position {
            x: 2875.0,
            y: 115.0,
          },
          outline_width: 1.0,
          outline_color: "#000".into(),
          fill_color: "#a22".into(),
        },
      ],
    }
  }

  pub fn width(&self) -> f64 {
    self.width
  }

  pub fn height(&self) -> f64 {
    self.height
  }

  pub fn constrain_circle_within_bounds(&self, center: Position, radius: f64) -> Position {
    Position {
      x: center.x.min(self.width - radius).max(0.0 + radius),
      y: center
        .y
        .min(self.height - Player::radius())
        .max(0.0 + Player::radius()),
    }
  }

  pub fn gen_tasks<T: FromIterator<Task>>(&self, settings: &Settings) -> T {
    // In the future, maps may want to have more of an opinion about the
    // kinds of tasks generated.
    (0..settings.num_tasks)
      .map(|_| loop {
        let mut rng = rand::thread_rng();
        let position = Position {
          x: rng.gen_range(30.0, self.width - 30.0),
          y: rng.gen_range(30.0, self.height - 30.0),
        };
        if self
          .static_geometry
          .iter()
          .any(|s| s.collides_with(position, 30.0))
        {
          continue;
        }
        return Task {
          finished: false,
          position,
        };
      })
      .collect()
  }

  pub fn get_spawn_in_position(&self) -> Position {
    let starting_position_seed: f64 = rand::random();
    let (circle, radius) = self.start_circle;
    Position {
      x: circle.x + (radius * (starting_position_seed * 2.0 * std::f64::consts::PI).sin()),
      y: circle.y + (radius * (starting_position_seed * 2.0 * std::f64::consts::PI).cos()),
    }
  }

  pub fn place_players_at_game_start(&self, players: &mut BTreeMap<UUID, Player>) {
    self.place_players_around_circle(players, self.start_circle);
  }

  pub fn place_players_at_night_start(&self, players: &mut BTreeMap<UUID, Player>) {
    self.place_players_around_circle(players, self.meeting_circle);
  }

  fn place_players_around_circle(
    &self,
    players: &mut BTreeMap<UUID, Player>,
    (center, radius): (Position, f64),
  ) {
    let num_players = players.len() as f64;
    for (i, (_, p)) in players.iter_mut().enumerate() {
      let offset = ((i as f64) / num_players) * 2.0 * std::f64::consts::PI;
      p.position = Position {
        x: center.x + (radius * offset.sin()),
        y: center.y + (radius * offset.cos()),
      };
      p.velocity = Velocity::default();
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Shape {
  Circle {
    radius: f64,
    center: Position,
    fill_color: String,
    outline_width: f64,
    outline_color: String,
  },
}

impl Shape {
  pub fn collide(
    &self,
    center: Position,
    radius: f64,
    movement_vector: Velocity,
    friction: f64,
  ) -> Velocity {
    match self {
      Shape::Circle {
        radius: self_radius,
        center: self_center,
        ..
      } => {
        // Circle on circle collision
        // https://www.gamasutra.com/view/feature/131424/pool_hall_lessons_fast_accurate_.php?print=1

        // Quick check to see whether, given starting locations and the magnitude of the
        // movement these two could collide if the player moved directly at this shape.
        let sum_radii = radius + self_radius;
        let dist = center.distance(self_center) - sum_radii;
        if movement_vector.magnitude() < dist {
          // Too far away, early exit.
          return movement_vector;
        }

        let n: Velocity = movement_vector.normalize();

        // Determine if A is moving towards B. If not, they're not colliding
        let c = self_center.sub(center);
        let d = n.dot_product(&c);
        if d <= 0.0 {
          return movement_vector;
        }

        let length_c = c.magnitude();
        let f = (length_c * length_c) - (d * d);
        let radii_squared = sum_radii * sum_radii;
        if f >= radii_squared {
          // the closest point on the movement vector is still too far away to collide
          return movement_vector;
        }

        let t = radii_squared - f;
        if t < 0.0 {
          return movement_vector;
        }

        let t_sqrt = t.sqrt();
        let distance = d - t_sqrt;
        let movement_magnitude = movement_vector.magnitude();

        if movement_magnitude < distance {
          return movement_vector;
        }

        let new_velocity = n.times(distance);

        let new_position = center.move_by(new_velocity);
        let vector_between_centers: Velocity = self_center.minus(&new_position);
        let angle = n.angle_between(&vector_between_centers);
        let angle_multiplier = angle * 2.0 / PI;

        let slope_of_tangent_line = -(1.0 / self_center.slope(&new_position));
        let tangent_point: Position = vector_between_centers
          .normalize::<Position>()
          .times(*self_radius);
        let second_tangent_point = Position {
          x: tangent_point.x + 1.0,
          y: tangent_point.y + slope_of_tangent_line,
        };
        let tangent_vector: Velocity = tangent_point.sub(second_tangent_point).normalize();
        let reversed_tangent_vector: Velocity = tangent_vector.times(-1.0);
        let tangent_vector = if tangent_vector.distance(&n) < reversed_tangent_vector.distance(&n) {
          tangent_vector
        } else {
          reversed_tangent_vector
        };

        let leftover_magnitude = movement_magnitude - distance;
        return new_velocity.add(
          &tangent_vector
            .times::<Velocity>(leftover_magnitude * angle_multiplier * (1.0 - friction)),
        );
      }
    }
  }

  pub fn collides_with(&self, other_position: Position, other_radius: f64) -> bool {
    match self {
      Shape::Circle { radius, center, .. } => {
        let distance = center.distance(&other_position);
        let collided = distance < (radius + other_radius);
        if collided {
          console_log!("{:?} collided with {:?}", center, other_position);
        }
        collided
      }
    }
  }
}

// We don't use a real UUID impl because getting randomness in the browser
// is different than the server, and I got a compiler error about it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UUID {
  v: [u8; 16],
}

// Display UUIDs as hex
impl Display for UUID {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    for byte in self.v.iter() {
      write!(fmt, "{:02x?}", byte)?;
    }
    Ok(())
  }
}

impl Debug for UUID {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    for byte in self.v.iter() {
      write!(fmt, "{:02x?}", byte)?;
    }
    Ok(())
  }
}

impl UUID {
  pub fn random() -> UUID {
    UUID { v: rand::random() }
  }
}

impl Serialize for UUID {
  fn serialize<S>(
    &self,
    serializer: S,
  ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
  where
    S: serde::Serializer,
  {
    serializer.serialize_str(&format!("{}", self))
  }
}
struct UUIDVisitor;
impl<'de> Visitor<'de> for UUIDVisitor {
  type Value = UUID;
  fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
    formatter.write_str("a 32 character long hex string")
  }

  fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    if value.len() != 32 {
      return Err(E::custom(format!(
        "expected UUID string to be len 32, was {}",
        value.len()
      )));
    }
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
      let si = i * 2;
      let hex_byte = &value[si..si + 2];
      *byte = match u8::from_str_radix(hex_byte, 16) {
        Ok(v) => v,
        Err(_) => {
          return Err(E::custom(format!(
            "expected hex, but found {} at offset {}",
            hex_byte, si
          )))
        }
      }
    }
    Ok(UUID { v: bytes })
  }
}
impl<'de> Deserialize<'de> for UUID {
  fn deserialize<D>(deserializer: D) -> Result<UUID, D::Error>
  where
    D: Deserializer<'de>,
  {
    deserializer.deserialize_str(UUIDVisitor)
  }
}

pub trait Vector2d {
  #[must_use]
  fn x(&self) -> f64;
  #[must_use]
  fn y(&self) -> f64;

  #[must_use]
  fn make_from_point(x: f64, y: f64) -> Self;

  #[must_use]
  fn distance(&self, other: &impl Vector2d) -> f64 {
    ((self.x() - other.x()).powi(2) + (self.y() - other.y()).powi(2))
      .sqrt()
      .abs()
  }

  #[must_use]
  fn magnitude(&self) -> f64 {
    self.distance(&Position { x: 0.0, y: 0.0 })
  }

  #[must_use]
  fn dot_product(&self, other: &impl Vector2d) -> f64 {
    (self.x() * other.x()) + (self.y() * other.y())
  }

  #[must_use]
  fn normalize<Ret: Vector2d>(&self) -> Ret {
    let magnitude = self.magnitude();
    Ret::make_from_point(self.x() / magnitude, self.y() / magnitude)
  }

  #[must_use]
  fn times<Ret: Vector2d>(&self, scalar: f64) -> Ret {
    Ret::make_from_point(self.x() * scalar, self.y() * scalar)
  }

  #[must_use]
  fn add<Ret: Vector2d>(&self, summand: &impl Vector2d) -> Ret {
    Ret::make_from_point(self.x() + summand.x(), self.y() + summand.y())
  }

  #[must_use]
  fn minus<Ret: Vector2d>(&self, subtrahend: &impl Vector2d) -> Ret {
    Ret::make_from_point(self.x() - subtrahend.x(), self.y() - subtrahend.y())
  }

  #[must_use]
  fn angle_between(&self, other: &impl Vector2d) -> f64 {
    (self.dot_product(other) / (self.magnitude() * other.magnitude())).acos()
  }

  #[must_use]
  fn slope(&self, other: &impl Vector2d) -> f64 {
    (other.y() - self.y()) / (other.x() - self.x())
  }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Position {
  pub x: f64,
  pub y: f64,
}

impl Vector2d for Position {
  fn x(&self) -> f64 {
    self.x
  }
  fn y(&self) -> f64 {
    self.y
  }

  fn make_from_point(x: f64, y: f64) -> Self {
    Self { x, y }
  }
}

impl Position {
  pub fn sub(self, other: Position) -> impl Vector2d {
    Position {
      x: self.x - other.x,
      y: self.y - other.y,
    }
  }

  pub fn move_by(self, velocity: Velocity) -> Position {
    Position {
      x: self.x + velocity.dx,
      y: self.y + velocity.dy,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct StartInfo {
  pub assignments: Vec<(UUID, PlayerStartInfo)>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Velocity {
  pub dx: f64,
  pub dy: f64,
}

impl Vector2d for Velocity {
  fn x(&self) -> f64 {
    self.dx
  }

  fn y(&self) -> f64 {
    self.dy
  }

  fn make_from_point(x: f64, y: f64) -> Self {
    Self { dx: x, dy: y }
  }
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
  Yellow,
  Purple,
  Gray,
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
      Color::Yellow,
      Color::Purple,
      Color::Gray,
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
      Color::Yellow => "#ffff66",
      Color::Purple => "#8a2be2",
      Color::Gray => "#333333",
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
impl Task {}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Player {
  pub name: String,
  pub uuid: UUID,
  pub color: Color,
  pub position: Position,
  pub dead: bool,
  pub impostor: bool,
  pub tasks: Vec<Task>,
  pub velocity: Velocity,
}

impl Player {
  pub fn new(uuid: UUID, name: String, color: Color, position: Position) -> Player {
    Player {
      name,
      uuid,
      color,
      dead: false,
      position,
      impostor: false,
      tasks: vec![],
      velocity: Velocity::default(),
    }
  }

  pub fn eligable_to_vote(&self) -> bool {
    !self.dead
  }

  pub fn radius() -> f64 {
    10.0
  }

  pub fn vision(&self, settings: &Settings) -> f64 {
    if self.impostor {
      settings.impostor_vision
    } else {
      settings.crew_vision
    }
  }

  pub fn can_see(&self, settings: &Settings, other: &Position) -> bool {
    let distance = self.position.distance(other);
    distance <= self.vision(settings)
  }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct DeadBody {
  pub color: Color,
  pub position: Position,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum GameStatus {
  Connecting,
  Lobby,
  Playing(PlayState),
  Won(Team),
  Disconnected,
}

impl GameStatus {
  pub fn progress_time(&mut self, elapsed: Duration) {
    if let GameStatus::Playing(PlayState::Day(day_state)) = self {
      day_state.time_remaining = day_state
        .time_remaining
        .checked_sub(elapsed)
        .unwrap_or_else(|| Duration::from_secs(0));
    }
  }

  pub fn is_same_kind(&self, other: &GameStatus) -> bool {
    match self {
      GameStatus::Connecting => matches!(other, GameStatus::Connecting),
      GameStatus::Lobby => matches!(other, GameStatus::Lobby),
      GameStatus::Playing(_) => matches!(other, GameStatus::Playing(_)),
      GameStatus::Won(_) => matches!(other, GameStatus::Won(_)),
      GameStatus::Disconnected => matches!(other, GameStatus::Disconnected),
    }
  }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum PlayState {
  Night,
  Day(DayState),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct DayState {
  pub votes: BTreeMap<UUID, VoteTarget>,
  pub time_remaining: Duration,
}

impl DayState {
  pub fn determine_winner_of_election(&self) -> VoteTarget {
    // Count the votes by the target.
    let mut vote_count: BTreeMap<VoteTarget, u16> = BTreeMap::new();
    for (_, target) in self.votes.iter() {
      *vote_count.entry(*target).or_insert(0) += 1;
    }
    // The winner is the one with the most votes!
    let mut targets_and_votes = vote_count.iter().collect::<Vec<_>>();
    targets_and_votes.sort_by_key(|(_target, count)| *count);
    if let Some((winner, winner_votes)) = targets_and_votes.get(0) {
      if let Some((_runner_up, runner_up_votes)) = targets_and_votes.get(1) {
        if runner_up_votes == winner_votes {
          // In case of a tie, skip
          return VoteTarget::Skip;
        }
      }
      return **winner;
    }
    // If no one voted, it's skip.
    VoteTarget::Skip
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum VoteTarget {
  Player { uuid: UUID },
  Skip,
}

impl GameStatus {
  pub fn finished(&self) -> bool {
    match self {
      GameStatus::Connecting => false,
      GameStatus::Lobby => false,
      GameStatus::Playing(_) => false,
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
