use core::fmt::Debug;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Display;

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
    fn random() -> UUID {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientToServerMessage {
    Move(MoveMessage),
    Killed(DeadBody),
    FinishedTask(FinishedTask),
    Join(Player),
    StartGame(StartGame),
}

impl ClientToServerMessage {
    #[allow(dead_code)]
    fn kind(&self) -> &'static str {
        match self {
            ClientToServerMessage::Move(_) => "Move",
            ClientToServerMessage::Killed(_) => "Killed",
            ClientToServerMessage::FinishedTask(_) => "FinishedTask",
            ClientToServerMessage::Join(_) => "Join",
            ClientToServerMessage::StartGame(_) => "StartGame",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerToClientMessage {
    Snapshot(Snapshot),
}

impl ServerToClientMessage {
    #[allow(dead_code)]
    fn kind(&self) -> &'static str {
        match self {
            ServerToClientMessage::Snapshot(_) => "Snapshot",
        }
    }
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

// A game from the perspective of a specific player
pub struct GameAsPlayer {
    my_uuid: UUID,
    inputs: InputState,
    pub state: GameState,
    socket: Box<dyn GameTx>,
}

pub trait GameTx {
    fn send(&self, message: &ClientToServerMessage) -> Result<(), String>;
}

// A game from the perspective of a particular player.
impl GameAsPlayer {
    pub fn new(socket: Box<dyn GameTx>) -> GameAsPlayer {
        let my_uuid = UUID::random();
        let starting_position_seed: f64 = rand::random();
        let local_player = Player {
            uuid: my_uuid,
            color: Color::random(),
            dead: false,
            position: Position {
                x: 275.0 + (100.0 * (starting_position_seed * 2.0 * std::f64::consts::PI).sin()),
                y: 275.0 + (100.0 * (starting_position_seed * 2.0 * std::f64::consts::PI).cos()),
            },
            impostor: false,
            // 6 random tasks
            tasks: vec![],
            speed: Speed { dx: 0.0, dy: 0.0 },
        };

        GameAsPlayer {
            state: GameState::new(vec![local_player]),
            inputs: InputState {
                up: false,
                down: false,
                left: false,
                right: false,
                kill: false,
                activate: false,
                report: false,
                play: false,
            },
            my_uuid,
            socket,
        }
    }

    // Is there a way to avoid duplicating the logic between local_player and local_player_mut?
    pub fn local_player(&self) -> Option<&Player> {
        self.state.get_player(self.my_uuid)
    }

    fn local_player_mut(&mut self) -> Option<&mut Player> {
        self.state.get_player_mut(self.my_uuid)
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
            self.state.status == GameStatus::Lobby && !current_input.play && new_input.play;
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
            self.socket.send(&ClientToServerMessage::Move(MoveMessage {
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
            dy = -self.state.speed
        } else if self.inputs.down {
            dy = self.state.speed
        }
        if self.inputs.left && !self.inputs.right {
            dx = -self.state.speed
        } else if self.inputs.right {
            dx = self.state.speed
        }
        Speed { dx, dy }
    }

    fn kill_player_near(&mut self, position: Position) -> Result<(), String> {
        let mut killed_player: Option<DeadBody> = None;
        let mut closest_distance = self.state.kill_distance;

        for player in self.state.players.iter_mut() {
            if player.impostor || player.uuid == self.my_uuid || player.dead {
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
            self.state.note_death(body)?;
            self.socket.send(&ClientToServerMessage::Killed(body))?;
            // Move the killer on top of the new body.
            if let Some(player) = self.local_player_mut() {
                player.position = body.position;
            }
        }

        Ok(())
    }

    fn activate_near(&mut self, position: Position) -> Result<(), String> {
        let mut closest_distance = self.state.task_distance;
        let local_player = match self.local_player_mut() {
            Some(player) => player,
            None => return Ok(()),
        };
        let is_imp = local_player.impostor;

        let mut finished_task: Option<FinishedTask> = None;
        for (index, task) in local_player.tasks.iter_mut().enumerate() {
            let distance = position.distance(task.position);
            if distance < closest_distance {
                finished_task = Some(FinishedTask { index });
                closest_distance = distance;
            }
        }
        if let Some(finished_task) = finished_task {
            if !is_imp {
                self.state.note_finished_task(self.my_uuid, finished_task)?;
                self.socket
                    .send(&ClientToServerMessage::FinishedTask(finished_task))?;
            }
        }
        Ok(())
    }

    pub fn connected(&mut self) -> Result<(), String> {
        self.state.status = GameStatus::Lobby;
        self.socket.send(&ClientToServerMessage::Join(
            self.local_player()
                .expect("Internal error: could not get local player during init")
                .clone(),
        ))
    }

    pub fn disconnected(&mut self) -> Result<(), String> {
        match self.state.status {
            GameStatus::Won(_) => (), // do nothing, this is expected
            _ => self.state.status = GameStatus::Disconnected,
        };
        Ok(())
    }

    pub fn handle_msg(&mut self, message: ServerToClientMessage) -> Result<(), String> {
        if self.state.status.finished() {
            return Ok(()); // Nothing more to say. Refresh for a new game!
        }
        match message {
            ServerToClientMessage::Snapshot(Snapshot {
                status,
                bodies,
                players,
            }) => {
                println!("{:?} received snapshot.", self.my_uuid);
                self.state.status = status;
                self.state.bodies = bodies;
                self.state.players = players;
            }
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), String> {
        // todo, pick this on the server
        let impostor_index = rand::thread_rng().gen_range(0, self.state.players.len());
        let impostor = &self.state.players[impostor_index];
        let impostors = vec![impostor.uuid];
        let start_data = StartGame { impostors };
        self.state.note_game_started(&start_data)?;
        self.socket
            .send(&ClientToServerMessage::StartGame(start_data))?;
        Ok(())
    }
}

// The full game state
#[derive(PartialEq, Clone, Debug)]
pub struct GameState {
    pub status: GameStatus,
    pub speed: f64,
    pub kill_distance: f64,
    pub task_distance: f64,
    pub players: Vec<Player>,
    pub bodies: Vec<DeadBody>,
}

impl GameState {
    pub fn new(players: Vec<Player>) -> GameState {
        GameState {
            status: GameStatus::Connecting,
            speed: 2.0,
            task_distance: 32.0,
            kill_distance: 64.0,
            players,
            bodies: Vec::new(),
        }
    }

    fn get_player(&self, uuid: UUID) -> Option<&Player> {
        self.players.iter().find(|p| p.uuid == uuid)
    }

    fn get_player_mut(&mut self, uuid: UUID) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.uuid == uuid)
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

    fn note_finished_task(
        &mut self,
        player_uuid: UUID,
        finished: FinishedTask,
    ) -> Result<(), String> {
        for player in self.players.iter_mut() {
            if player.uuid == player_uuid {
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
}

// Implements logic for a game server without knowing about the transport layer.
// Useful so that we can implement a real game server with web sockets, and the test
// game server, and potentially a future peer to peer in-client server.
pub struct GameServer {
    state: GameState,
    broadcaster: Box<dyn Broadcaster>,
}

pub trait Broadcaster: Send {
    fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>>;
    fn send_to_player(
        &self,
        uuid: &UUID,
        message: &ServerToClientMessage,
    ) -> Result<(), Box<dyn Error>>;
}

impl GameServer {
    pub fn new(broadcaster: Box<dyn Broadcaster>) -> GameServer {
        let mut gs = GameServer {
            state: GameState::new(vec![]),
            broadcaster,
        };
        gs.state.status = GameStatus::Lobby;
        gs
    }

    pub fn simulate(&mut self, elapsed: f64) {
        self.state.simulate(elapsed);
    }

    pub fn disconnected(&mut self, disconnected_player: UUID) -> Result<(), Box<dyn Error>> {
        self.state.players.retain(|p| p.uuid != disconnected_player);
        self.broadcast_snapshot()?;
        Ok(())
    }

    pub fn handle_message(
        &mut self,
        sender: UUID,
        message: ClientToServerMessage,
    ) -> Result<(), Box<dyn Error>> {
        println!("Game server handling {:?}", message);
        match message {
            ClientToServerMessage::StartGame(start) => {
                self.state.note_game_started(&start)?;
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
                for player in self.state.players.iter_mut() {
                    if player.uuid == sender {
                        player.speed = moved.speed;
                        player.position = moved.position;
                    }
                }
                self.broadcast_snapshot()?;
            }
            ClientToServerMessage::Join(mut player) => {
                if self.state.status == GameStatus::Lobby {
                    for p in self.state.players.iter() {
                        if p.uuid == player.uuid {
                            return Ok(()); // we know about this player already
                        }
                    }
                    // ok, it's a new player, and we have room for them. if their color is
                    // already taken, give them a new one.
                    let taken_colors: BTreeSet<Color> =
                        self.state.players.iter().map(|p| p.color).collect();
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
                        self.state.players.push(player);
                    }
                }

                // Send out a snapshot to catch the new client up, whether or not they're playing.
                self.broadcast_snapshot()?;
            }
        };
        Ok(())
    }

    fn broadcast_snapshot(&self) -> Result<(), Box<dyn Error>> {
        self.broadcaster
            .broadcast(&ServerToClientMessage::Snapshot(Snapshot {
                status: self.state.status,
                bodies: self.state.bodies.clone(),
                players: self.state.players.clone(),
            }))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    #[test]
    fn pythagoras_was_right() {
        assert!(
            (5.0 - (Position { x: 3.0, y: 0.0 }).distance(Position { x: 0.0, y: 4.0 })).abs()
                < 0.01
        );
    }

    struct TestEnvironment {
        game_server: GameServer,
        server_to_client_queue: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>>,
        players: HashMap<UUID, GameAsPlayer>,
        player_queue: HashMap<UUID, Arc<Mutex<Vec<ClientToServerMessage>>>>,
    }
    impl TestEnvironment {
        fn new() -> TestEnvironment {
            let messages: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>> = Arc::default();
            TestEnvironment {
                game_server: GameServer::new(Box::new(TestBroadcaster {
                    players: messages.clone(),
                })),
                server_to_client_queue: messages,
                players: HashMap::default(),
                player_queue: HashMap::default(),
            }
        }

        // This creates a player, and establishes a bidirectional connection in our
        // fake network, but doesn't send any messages.
        fn create_player(&mut self) -> UUID {
            let queue: Arc<Mutex<Vec<ClientToServerMessage>>> = Arc::default();
            let player = GameAsPlayer::new(Box::new(TestPlayerTx {
                messages: queue.clone(),
            }));
            let uuid = player.my_uuid;
            self.server_to_client_queue
                .lock()
                .unwrap()
                .insert(uuid, vec![]);
            self.players.insert(uuid, player);
            self.player_queue.insert(uuid, queue);
            uuid
        }

        // This removes a player from the fake network. Because a server would
        // also notice the disconnection separate from any message, it also
        // notifies the server of the disconnection.
        fn remove_player(&mut self, uuid: UUID) -> Result<GameAsPlayer, Box<dyn Error>> {
            self.server_to_client_queue
                .lock()
                .unwrap()
                .remove(&uuid)
                .ok_or("Not in server_to_client_queue")?;
            self.player_queue
                .remove(&uuid)
                .ok_or("not in player_queue")?;
            let result = self.players.remove(&uuid).ok_or("not in players")?;
            self.game_server.disconnected(uuid)?;
            Ok(result)
        }

        // Pass messages from server to client and client to server until we
        // reach a steady state where no one has any more messages for anyone else.
        fn dispatch_messages(&mut self) -> Result<(), Box<dyn Error>> {
            let mut keep_going = true;
            while keep_going {
                keep_going = false;
                for (uuid, messages_lock) in self.player_queue.iter_mut() {
                    let messages: Vec<ClientToServerMessage>;
                    {
                        let mut messages_local = messages_lock.lock().unwrap();
                        if messages_local.is_empty() {
                            continue;
                        }
                        keep_going = true;
                        messages = messages_local.clone();
                        messages_local.clear();
                    }
                    for message in messages {
                        println!("Passing {} to server from {}", message.kind(), uuid);
                        self.game_server.handle_message(*uuid, message)?;
                    }
                }
                let mut queue = self.server_to_client_queue.lock().unwrap();
                println!(
                    "Processing {} players in server_to_client_queue",
                    queue.len()
                );
                for (uuid, messages) in queue.iter_mut() {
                    if messages.is_empty() {
                        println!("Messages to {} are empty", uuid);
                        continue;
                    }
                    keep_going = true;
                    let messages_local = messages.clone();
                    for message in messages_local {
                        println!("Passing {} from server to {}", message.kind(), uuid);
                        let player = self.players.get_mut(uuid).unwrap();
                        player.handle_msg(message)?;
                    }
                    messages.clear();
                }
                println!(
                    "Finished one loop of dispatch_messages. Keep going? {}",
                    keep_going
                );
            }
            Ok(())
        }

        fn expect_everyone_agrees_on_game_state(
            &self,
            expected_num_players: usize,
        ) -> Result<GameState, Box<dyn Error>> {
            let server_state = self.game_server.state.clone();
            assert_eq!(self.players.len(), expected_num_players);
            for (_, game_as_player) in self.players.iter() {
                // Will eventually need to customize this more, because players will
                // only know a subset.
                assert_eq!(&server_state, &game_as_player.state);
            }
            Ok(server_state)
        }

        fn time_passes(&mut self, elapsed: f64) {
            self.game_server.state.simulate(elapsed);
            for (_, player) in self.players.iter_mut() {
                player.state.simulate(elapsed);
            }
        }

        fn create_and_connect_player(&mut self) -> Result<UUID, Box<dyn Error>> {
            let player_count = self.players.len();
            let id = self.create_player();
            self.players.get_mut(&id).unwrap().connected()?;
            self.dispatch_messages()?;
            self.expect_everyone_agrees_on_game_state(player_count + 1)?;
            Ok(id)
        }
    }

    struct TestBroadcaster {
        players: Arc<Mutex<HashMap<UUID, Vec<ServerToClientMessage>>>>,
    }
    impl Broadcaster for TestBroadcaster {
        fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
            println!("Broadcasting {} from server", message.kind());
            let mut players = self.players.lock().unwrap();
            for (_uuid, messages) in players.iter_mut() {
                messages.push(message.clone());
            }
            println!("Broadcast complete");
            Ok(())
        }
        fn send_to_player(
            &self,
            uuid: &UUID,
            message: &ServerToClientMessage,
        ) -> Result<(), Box<dyn Error>> {
            println!("Sending {} to player {} from server", message.kind(), uuid);
            let mut players = self.players.lock().unwrap();
            if let Some(messages) = players.get_mut(uuid) {
                messages.push(message.clone());
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestPlayerTx {
        messages: Arc<Mutex<Vec<ClientToServerMessage>>>,
    }
    impl GameTx for TestPlayerTx {
        fn send(&self, message: &ClientToServerMessage) -> Result<(), String> {
            self.messages.lock().unwrap().push((*message).clone());
            Ok(())
        }
    }

    #[test]
    fn test_connection_and_disconnection() -> Result<(), Box<dyn Error>> {
        let mut env = TestEnvironment::new();
        env.expect_everyone_agrees_on_game_state(0)?;

        // P1 connects
        let player1_id = env.create_player();
        let player1 = env.players.get_mut(&player1_id).unwrap();
        player1.connected()?;
        env.dispatch_messages()?;
        env.expect_everyone_agrees_on_game_state(1)?;

        // P2 connects
        let player2_id = env.create_player();
        let player2 = env.players.get_mut(&player2_id).unwrap();
        player2.connected()?;
        env.dispatch_messages()?;
        env.expect_everyone_agrees_on_game_state(2)?;

        // P3 connects
        let player3_id = env.create_player();
        let player3 = env.players.get_mut(&player3_id).unwrap();
        player3.connected()?;
        env.dispatch_messages()?;
        let game = env.expect_everyone_agrees_on_game_state(3)?;

        // P2 disconnects
        assert!(game.players.iter().any(|p| p.uuid == player2_id));
        env.remove_player(player2_id)?;
        env.dispatch_messages()?;
        let game = env.expect_everyone_agrees_on_game_state(2)?;
        assert!(!game.players.iter().any(|p| p.uuid == player2_id));

        Ok(())
    }

    #[test]
    fn test_movement() -> Result<(), Box<dyn Error>> {
        let mut env = TestEnvironment::new();
        env.expect_everyone_agrees_on_game_state(0)?;

        let player1_id = env.create_and_connect_player()?;
        let player2_id = env.create_and_connect_player()?;
        let player3_id = env.create_and_connect_player()?;

        // Move p1 up and left
        let p1_client = env.players.get_mut(&player1_id).unwrap();
        p1_client.take_input(InputState {
            up: true,
            left: true,
            right: false,
            down: false,
            activate: false,
            kill: false,
            play: false,
            report: false,
        })?;
        let p1_position = p1_client.local_player().unwrap().position;

        // Don't move p2.
        let p2_position = env
            .players
            .get_mut(&player2_id)
            .unwrap()
            .local_player()
            .unwrap()
            .position;

        // Move p3 down and right
        let p3_client = env.players.get_mut(&player3_id).unwrap();
        p3_client.take_input(InputState {
            up: false,
            left: false,
            right: true,
            down: true,
            activate: false,
            kill: false,
            play: false,
            report: false,
        })?;
        let p3_position = p3_client.local_player().unwrap().position;

        // Let four time ticks pass
        env.dispatch_messages()?;
        env.time_passes(64.0);

        let player_positions: HashMap<UUID, Position> = env
            .game_server
            .state
            .players
            .iter()
            .map(|p| (p.uuid, p.position))
            .collect();

        // P1 moved up and to the left
        let updated_p1_position = player_positions.get(&player1_id).unwrap();
        assert!(updated_p1_position.x < p1_position.x);
        assert!(updated_p1_position.y < p1_position.y);
        // P2's position hasn't changed
        assert_eq!(player_positions.get(&player2_id), Some(&p2_position));
        let updated_p3_position = player_positions.get(&player3_id).unwrap();
        // P3 moved down and to the right
        assert!(updated_p3_position.x > p3_position.x);
        assert!(updated_p3_position.y > p3_position.y);

        // Clients and server agree
        env.expect_everyone_agrees_on_game_state(3)?;

        Ok(())
    }
}
