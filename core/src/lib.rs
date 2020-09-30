use serde::{Deserialize, Serialize};

// We don't use a real UUID impl because getting randomness in the browser
// is different than the server, and I got a compiler error about it.
pub type UUID = [u8; 16];

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
        // Note: we assume this is sorted.
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

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
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
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Move(MoveMessage),
    Killed(DeadBody),
    FinishedTask(FinishedTask),
    Join(Player),
    Snapshot(Snapshot),
    StartGame(StartGame),
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct MoveMessage {
    pub color: Color,
    pub speed: Speed,
    pub position: Position,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct FinishedTask {
    pub color: Color,
    pub index: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub status: GameStatus,
    pub bodies: Vec<DeadBody>,
    pub players: Vec<Player>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StartGame {
    pub impostors: Vec<UUID>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pythagoras_was_right() {
        assert!(
            (5.0 - (Position { x: 3.0, y: 0.0 }).distance(Position { x: 0.0, y: 4.0 })).abs()
                < 0.01
        );
    }
}
