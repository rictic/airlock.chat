use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

// The state of user input at some point in time. i.e. what buttons is
// the user holding down?
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub kill: bool,
    pub activate: bool,
    pub report: bool,
    pub play: bool,
}

pub struct Game {
    pub status: GameStatus,
    pub speed: f64,
    pub kill_distance: f64,
    pub task_distance: f64,
    pub local_player_uuid: Option<UUID>,
    pub inputs: InputState,
    pub players: Vec<Player>,
    pub bodies: Vec<DeadBody>,
    pub socket: Box<dyn GameTx>,
}

pub trait GameTx {
    fn send(&self, message: &Message) -> Result<(), String>;
}

impl Game {
    // Is there a way to avoid duplicating this logic?
    pub fn local_player(&self) -> Option<&Player> {
        let local_player_uuid = self.local_player_uuid?;
        for player in self.players.iter() {
            if player.uuid == local_player_uuid {
                return Some(player);
            }
        }
        None
    }

    fn local_player_mut(&mut self) -> Option<&mut Player> {
        let local_player_uuid = self.local_player_uuid?;
        for player in self.players.iter_mut() {
            if player.uuid == local_player_uuid {
                return Some(player);
            }
        }
        None
    }

    fn simulate_internal(&mut self, elapsed: f64) -> Result<(), &'static str> {
        // elapsed is the time, in milliseconds, that has passed since the
        // last time we simulated.
        // By making our simulations relative to the amount of time that's passed,
        // the game will progress the same regardless of the frame rate, which may
        // vary between 30fps and 144fps even if our performance is perfect!
        let time_steps_passed = elapsed / 16.0;

        for player in self.players.iter_mut() {
            let Speed { dx, dy } = player.speed;
            player.position.x += dx * time_steps_passed;
            player.position.y += dy * time_steps_passed;
            // We don't handle inputs.q here because player position may be
            // out of sync, but we _super_ don't want to let life or death
            // get out of sync.
        }

        Ok(())
    }

    pub fn simulate(&mut self, elapsed: f64) -> Option<String> {
        let result = self.simulate_internal(elapsed);
        match result {
            Ok(()) => None,
            Err(s) => Some(s.to_string()),
        }
    }

    // Take the given inputs from the local player
    pub fn take_input(&mut self, new_input: InputState) -> Result<(), String> {
        let current_input = self.inputs;
        let player = match self.local_player_mut() {
            None => return Ok(()),
            Some(p) => p,
        };
        if new_input == current_input {
            return Ok(()); // quick exit for the boring case
        }
        // Read the parts of the local player that we care about.
        let is_killing = player.impostor && !current_input.kill && new_input.kill;
        let position = player.position;
        let activating = !current_input.activate && new_input.activate;
        let starting_play =
            self.status == GameStatus::Lobby && !current_input.play && new_input.play;
        self.inputs = new_input;
        // ok, we're done touching player at this point. we redeclare it
        // below so we can use it again, next time mutably.

        if is_killing {
            self.kill_player_near(position)?;
        }
        if activating {
            self.activate_near(position)?;
        }
        if starting_play {
            self.start()?;
        }

        let speed_changed: bool;
        {
            let new_speed = self.get_speed();
            let player = self.local_player_mut().unwrap();
            speed_changed = new_speed != player.speed;
            player.speed = new_speed;
        }

        // This way we don't send a MoveMessage unless movement keys actually changed,
        // reducing data leakage to HAXXORZ.
        if speed_changed {
            let player = self.local_player().unwrap();
            self.send_msg(&Message::Move(MoveMessage {
                color: player.color,
                speed: player.speed,
                position: player.position,
            }))?;
        }
        Ok(())
    }

    fn get_speed(&self) -> Speed {
        let mut dx = 0.0;
        let mut dy = 0.0;
        if self.inputs.up && !self.inputs.down {
            dy = -self.speed
        } else if self.inputs.down {
            dy = self.speed
        }
        if self.inputs.left && !self.inputs.right {
            dx = -self.speed
        } else if self.inputs.right {
            dx = self.speed
        }
        Speed { dx, dy }
    }

    fn kill_player_near(&mut self, position: Position) -> Result<(), String> {
        let local_player_uuid = match &self.local_player_uuid {
            None => return Ok(()), // not controlling anything
            Some(c) => *c,
        };

        let mut killed_player: Option<DeadBody> = None;
        let mut closest_distance = self.kill_distance;

        for player in self.players.iter_mut() {
            if player.impostor || player.uuid == local_player_uuid || player.dead {
                continue;
            }

            let distance = position.distance(player.position);
            if distance < closest_distance {
                killed_player = Some(DeadBody {
                    position: player.position,
                    color: player.color,
                });
                closest_distance = distance;
            }
        }

        if let Some(body) = killed_player {
            self.note_death(body)?;
            self.send_msg(&Message::Killed(body))?;
            // Move the killer on top of the new body.
            if let Some(player) = self.local_player_mut() {
                player.position = body.position;
            }
        }

        Ok(())
    }

    fn activate_near(&mut self, position: Position) -> Result<(), String> {
        let mut closest_distance = self.task_distance;
        let local_player = match self.local_player_mut() {
            Some(player) => player,
            None => return Ok(()),
        };
        let is_imp = local_player.impostor;
        let color = local_player.color;

        let mut finished_task: Option<FinishedTask> = None;
        for (index, task) in local_player.tasks.iter_mut().enumerate() {
            let distance = position.distance(task.position);
            if distance < closest_distance {
                finished_task = Some(FinishedTask { index, color });
                closest_distance = distance;
            }
        }
        if let Some(finished_task) = finished_task {
            if !is_imp {
                self.note_finished_task(finished_task)?;
                self.send_msg(&Message::FinishedTask(finished_task))?;
            }
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), String> {
        // todo, pick this on the server
        let impostor_index = 0;
        let impostor = &self.players[impostor_index];
        let impostors = vec![impostor.uuid];
        let start_data = StartGame { impostors };
        self.note_game_started(&start_data)?;
        self.send_msg(&Message::StartGame(start_data))?;
        Ok(())
    }

    fn win(&mut self, team: Team) -> Result<(), String> {
        self.status = GameStatus::Won(team);
        Ok(())
    }

    fn note_game_started(&mut self, start_data: &StartGame) -> Result<(), String> {
        if self.status != GameStatus::Lobby {
            return Err(format!("Internal error: got a message to start a game when not in the lobby!? Game status: {:?}", self.status));
        }
        self.status = GameStatus::Playing;
        for player in self.players.iter_mut() {
            for impostor_uuid in start_data.impostors.iter() {
                if player.uuid == *impostor_uuid {
                    player.impostor = true;
                }
            }
        }
        Ok(())
    }

    fn note_death(&mut self, body: DeadBody) -> Result<(), String> {
        for player in self.players.iter_mut() {
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
        for player in self.players.iter() {
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

    fn note_finished_task(&mut self, finished: FinishedTask) -> Result<(), String> {
        for player in self.players.iter_mut() {
            if player.color == finished.color {
                if let Some(task) = player.tasks.get_mut(finished.index) {
                    task.finished = true;
                }
            }
        }
        let all_crew_tasks_finished = self
            .players
            .iter()
            .filter(|p| !p.impostor)
            .all(|p| p.tasks.iter().all(|t| t.finished));
        if all_crew_tasks_finished {
            self.win(Team::Crew)?;
        }
        Ok(())
    }

    pub fn send_msg(&self, message: &Message) -> Result<(), String> {
        self.socket.send(&message)
    }

    pub fn handle_msg(&mut self, message: Message) -> Result<(), String> {
        if self.status.finished() {
            return Ok(()); // Nothing more to say. Refresh for a new game!
        }
        match message {
            Message::StartGame(start) => self.note_game_started(&start)?,
            Message::Killed(body) => self.note_death(body)?,
            Message::FinishedTask(finished) => self.note_finished_task(finished)?,
            Message::Move(moved) => {
                for player in self.players.iter_mut() {
                    if player.color == moved.color {
                        player.speed = moved.speed;
                        player.position = moved.position;
                    }
                }
            }
            Message::Snapshot(Snapshot {
                status,
                bodies,
                players,
            }) => {
                // This is a sloppy kind of consensus that isn't guaranteed to converge.
                // Easiest way to resolve this is to make the server smarter, but that will
                // involve learning tokio D:
                self.status = status;
                self.bodies = bodies;
                self.players = players;
            }
            Message::Join(mut player) => {
                if self.status == GameStatus::Lobby {
                    for p in self.players.iter() {
                        if p.uuid == player.uuid {
                            return Ok(()); // we know about this player already
                        }
                    }
                    // ok, it's a new player, and we have room for them. if their color is
                    // already taken, give them a new one.
                    let taken_colors: BTreeSet<Color> =
                        self.players.iter().map(|p| p.color).collect();
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
                        self.players.push(player);
                    }
                }

                // Send out a snapshot to catch the new client up, whether or not they're playing.
                self.send_msg(&Message::Snapshot(Snapshot {
                    status: self.status,
                    bodies: self.bodies.clone(),
                    players: self.players.clone(),
                }))?;
            }
        }
        Ok(())
    }
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
