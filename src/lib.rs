mod utils;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::f64;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
enum Color {
    Red,
    Pink,
    Blue,
    Orange,
    White,
    Black,
    Green,
}

impl Color {
    fn all() -> &'static [Color] {
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
    fn to_str(&self) -> &'static str {
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

    fn as_js_value(&self) -> JsValue {
        JsValue::from_str(self.to_str())
    }

    fn as_semitransparant_js_value(&self) -> JsValue {
        JsValue::from(format!("{}88", self.to_str()))
    }

    fn random() -> Color {
        let idx = (js_sys::Math::random() * (Color::all().len() as f64)).floor() as usize;
        Color::all()[idx]
    }
}

type UUID = [u8; 16];

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct Player {
    uuid: UUID,
    color: Color,
    position: Position,
    dead: bool,
    impostor: bool,
    tasks: Vec<Task>,
    speed: Speed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
struct Position {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
struct Speed {
    dx: f64,
    dy: f64,
}

impl Position {
    fn distance(self, other: Position) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2))
            .sqrt()
            .abs()
    }
}

#[wasm_bindgen]
#[derive(Clone)]
struct Game {
    speed: f64,
    width: f64,
    height: f64,
    kill_distance: f64,
    task_distance: f64,
    local_player_uuid: Option<UUID>,
    inputs: InputState,
    context: web_sys::CanvasRenderingContext2d,
    players: Vec<Player>,
    bodies: Vec<DeadBody>,
    socket: WebSocket,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
struct DeadBody {
    color: Color,
    position: Position,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
struct Task {
    position: Position,
    finished: bool,
}

// The state of user input at some point in time. i.e. what buttons is
// the user holding down?
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct InputState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    kill: bool,
    activate: bool,
    report: bool,
}

#[wasm_bindgen]
impl Game {
    fn draw_internal(&self) -> Result<(), Box<dyn Error>> {
        let context = &self.context;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        context.begin_path();
        context.rect(0.0, 0.0, self.width, self.height);
        context.set_fill_style(&JsValue::from_str("#f3f3f3"));
        context.fill();
        context.stroke();

        // Draw the conference table
        context.set_stroke_style(&JsValue::from_str("#000000"));
        context.set_fill_style(&JsValue::from_str("#358"));
        self.circle(275.0, 275.0, 75.0)?;

        let show_dead_people = match self.local_player() {
            None => true,
            Some(p) => p.dead || p.impostor,
        };

        // Draw bodies first, then players on top, so imps can stand on top of bodies.
        // However maybe we should instead draw items from highest to lowest, vertically?
        if let Some(local_player) = self.local_player() {
            for task in local_player.tasks.iter() {
                if task.finished {
                    continue;
                }
                self.draw_task(*task, local_player.impostor)?;
            }
        }
        for body in self.bodies.iter() {
            self.draw_body(*body)?;
        }
        for player in self.players.iter() {
            if show_dead_people || !player.dead {
                self.draw_player(player)?
            }
        }

        Ok(())
    }

    fn draw_player(&self, player: &Player) -> Result<(), &'static str> {
        self.context.begin_path();
        let radius = 10.0;
        self.context
            .move_to(player.position.x + radius, player.position.y);
        self.context
            .arc(
                player.position.x,
                player.position.y,
                radius,
                0.0,
                f64::consts::PI * 2.0,
            )
            .map_err(|_| "Failed to draw a circle.")?;
        let color = if player.dead {
            player.color.as_semitransparant_js_value()
        } else {
            player.color.as_js_value()
        };
        self.context.set_fill_style(&color);
        let stroke_color = if player.dead {
            JsValue::from("#00000088")
        } else {
            JsValue::from("#000000")
        };
        self.context.set_stroke_style(&stroke_color);
        self.context.fill();
        self.context.stroke();
        Ok(())
    }

    fn draw_body(&self, body: DeadBody) -> Result<(), &'static str> {
        self.context.begin_path();
        let radius = 10.0;
        self.context
            .move_to(body.position.x + radius, body.position.y);
        self.context
            .arc(
                body.position.x,
                body.position.y,
                radius,
                0.0,
                f64::consts::PI * 1.0,
            )
            .map_err(|_| "Failed to draw a circle.")?;
        self.context.set_fill_style(&body.color.as_js_value());
        self.context.set_stroke_style(&JsValue::from("#000000"));
        self.context.fill();
        self.context.stroke();
        Ok(())
    }

    fn draw_task(&self, task: Task, fake: bool) -> Result<(), &'static str> {
        self.context.begin_path();
        let len: f64 = 15.0;
        let pos = task.position;
        // drawing an equilateral triangle...
        let height = (len.powf(2.0) - (len / 2.0).powf(2.0)).sqrt();
        self.context.move_to(pos.x + (len / 2.0), pos.y);
        self.context.line_to(pos.x, pos.y + height);
        self.context.line_to(pos.x + len, pos.y + height);
        self.context.line_to(pos.x + (len / 2.0), pos.y);
        if fake {
            self.context.set_fill_style(&JsValue::from("#ffa50244"));
            self.context.set_stroke_style(&JsValue::from("#00000044"));
        } else {
            self.context.set_fill_style(&JsValue::from("#ffa502"));
            self.context.set_stroke_style(&JsValue::from("#000000"));
        }
        self.context.fill();
        self.context.stroke();
        self.context.move_to(pos.x + (len / 2.0), pos.y + 3.0);
        self.context.line_to(pos.x + (len / 2.0), pos.y + 9.0);
        self.context.move_to(pos.x + (len / 2.0), pos.y + 10.0);
        self.context.line_to(pos.x + (len / 2.0), pos.y + 12.0);
        self.context.stroke();
        Ok(())
    }

    fn circle(&self, x: f64, y: f64, radius: f64) -> Result<(), &'static str> {
        self.context.begin_path();
        self.context.move_to(x + radius, y);
        self.context
            .arc(x, y, radius, 0.0, f64::consts::PI * 2.0)
            .map_err(|_| "Failed to draw a circle.")?;
        self.context.stroke();
        self.context.fill();
        Ok(())
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

    // These params describe whether the player is currently holding down each
    // of the buttons.
    fn draw(&self) -> Option<String> {
        match self.draw_internal() {
            Ok(()) => None,
            Err(e) => Some(format!("Error: {}", e)),
        }
    }

    // Is there a way to avoid duplicating this logic?
    fn local_player(&self) -> Option<&Player> {
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

    fn simulate(&mut self, elapsed: f64) -> Option<String> {
        let result = self.simulate_internal(elapsed);
        match result {
            Ok(()) => None,
            Err(s) => Some(s.to_string()),
        }
    }

    // Take the given inputs from the local player
    fn take_input(&mut self, new_input: InputState) -> Result<(), JsValue> {
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
        self.inputs = new_input;
        // ok, we're done touching player at this point. we redeclare it
        // below so we can use it again, next time mutably.

        if is_killing {
            self.kill_player_near(position)?;
        }
        if activating {
            self.activate_near(position)?;
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

    fn kill_player_near(&mut self, position: Position) -> Result<(), JsValue> {
        let local_player_uuid = match &self.local_player_uuid {
            None => return Ok(()), // not controlling anything
            Some(c) => *c,
        };

        let mut killed_player: Option<&mut Player> = None;
        let mut closest_distance = self.kill_distance;

        for player in self.players.iter_mut() {
            if player.impostor || player.uuid == local_player_uuid || player.dead {
                continue;
            }

            let distance = position.distance(player.position);
            if distance < closest_distance {
                killed_player = Some(player);
                closest_distance = distance;
            }
        }

        if let Some(player) = killed_player {
            player.dead = true;
            let player: &Player = player;
            let body = DeadBody {
                position: player.position,
                color: player.color,
            };
            self.bodies.push(body);
            self.send_msg(&Message::Killed(body))?;
            // Move the killer on top of the new body.
            if let Some(player) = self.local_player_mut() {
                player.position = body.position;
            }
        }

        Ok(())
    }

    fn activate_near(&mut self, position: Position) -> Result<(), JsValue> {
        let mut closest_distance = self.kill_distance;
        let local_player = match self.local_player_mut() {
            Some(player) => player,
            None => return Ok(()),
        };
        let is_imp = local_player.impostor;
        let color = local_player.color;

        let mut finished_task: Option<(usize, &mut Task)> = None;
        for (i, task) in local_player.tasks.iter_mut().enumerate() {
            let distance = position.distance(task.position);
            if distance < closest_distance {
                finished_task = Some((i, task));
                closest_distance = distance;
            }
        }
        if let Some((i, task)) = finished_task {
            if !is_imp {
                task.finished = true;
                self.send_msg(&Message::FinishedTask(FinishedTask { color, index: i }))?;
            }
        }
        Ok(())
    }

    fn send_msg(&self, message: &Message) -> Result<(), JsValue> {
        let encoded = serde_json::to_string(&message)
            .map_err(|_| JsValue::from_str("Unable to encode Message to json"))?;
        self.socket.send_with_str(&encoded)?;
        Ok(())
    }

    fn handle_msg(&mut self, message: Message) -> Result<(), JsValue> {
        match message {
            Message::Killed(body) => {
                for player in self.players.iter_mut() {
                    if player.color == body.color {
                        player.dead = true;
                    }
                }
                self.bodies.push(body);
            }
            Message::Move(moved) => {
                for player in self.players.iter_mut() {
                    if player.color == moved.color {
                        player.speed = moved.speed;
                        player.position = moved.position;
                    }
                }
            }
            Message::FinishedTask(FinishedTask { color, index }) => {
                for player in self.players.iter_mut() {
                    if player.color == color {
                        if let Some(task) = player.tasks.get_mut(index) {
                            task.finished = true;
                        }
                    }
                }
            }
            Message::Snapshot(Snapshot { bodies, players }) => {
                // This is a sloppy kind of consensus that isn't guaranteed to converge.
                // Easiest way to resolve this is to make the server smarter, but that will
                // involve learning tokio D:
                self.bodies = bodies;
                self.players = players;
            }
            Message::Join(mut player) => {
                for p in self.players.iter() {
                    if p.uuid == player.uuid {
                        return Ok(()); // we know about this player already
                    }
                }
                // ok, it's a new player, and we have room for them. if their color is
                // already taken, give them a new one.
                let taken_colors: BTreeSet<Color> = self.players.iter().map(|p| p.color).collect();
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
                // Send out a snapshot to catch the new client up, whether or not they're playing.
                self.send_msg(&Message::Snapshot(Snapshot {
                    bodies: self.bodies.clone(),
                    players: self.players.clone(),
                }))?;
            }
        }
        Ok(())
    }
}

struct CanvasInfo {
    // The 2d rendering context
    context: web_sys::CanvasRenderingContext2d,
    // Dimensions of the canvas (for now we assume this won't change)
    width: f64,
    height: f64,
}

fn get_canvas_info() -> Result<CanvasInfo, Box<dyn Error>> {
    let document = web_sys::window()
        .ok_or("Could not get window")?
        .document()
        .ok_or("Could not get document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("Could not find element with id 'canvas'")?;
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "Element with id 'canvas' isn't a canvas element")?;

    let context = canvas
        .get_context("2d")
        .map_err(|_| "Could not get 2d canvas context")?
        .ok_or("Got null 2d canvas context")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "Returned value was not a CanvasRenderingContext2d")?;

    Ok(CanvasInfo {
        context,
        width: canvas.width().into(),
        height: canvas.height().into(),
    })
}

#[wasm_bindgen]
pub struct GameWrapper {
    game: Arc<Mutex<Game>>,
}

#[wasm_bindgen]
impl GameWrapper {
    #[allow(clippy::too_many_arguments)]
    pub fn set_inputs(
        &mut self,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        kill: bool,
        report: bool,
        activate: bool,
    ) -> Result<(), JsValue> {
        let mut game = self
            .game
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        game.take_input(InputState {
            up,
            down,
            left,
            right,
            kill,
            report,
            activate,
        })
    }

    pub fn simulate(&mut self, elapsed: f64) -> Result<Option<String>, JsValue> {
        let mut game = self
            .game
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        Ok(game.simulate(elapsed))
    }

    pub fn draw(&mut self) -> Result<Option<String>, JsValue> {
        let game = self
            .game
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        Ok(game.draw())
    }
}

pub fn random_byte() -> u8 {
    (js_sys::Math::random() * 256.0).floor() as u8
}

#[wasm_bindgen]
pub fn make_game() -> Result<GameWrapper, JsValue> {
    let CanvasInfo {
        context,
        width,
        height,
    } = get_canvas_info()
        .map_err(|e| JsValue::from(format!("Error initializing canvas: {}", e)))?;
    let ws = WebSocket::new("ws://localhost:3012")?;

    let my_uuid: UUID = [
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
        random_byte(),
    ];
    let starting_position_seed = js_sys::Math::random();
    let local_player = Player {
        uuid: my_uuid,
        color: Color::random(),
        dead: false,
        position: Position {
            x: 275.0 + (100.0 * (starting_position_seed * 2.0 * f64::consts::PI).sin()),
            y: 275.0 + (100.0 * (starting_position_seed * 2.0 * f64::consts::PI).cos()),
        },
        impostor: false,
        // 6 random tasks
        tasks: (0..6)
            .map(|_| Task {
                position: Position {
                    x: (js_sys::Math::random() * width)
                        .floor()
                        .max(30.0)
                        .min(width - 30.0),
                    y: (js_sys::Math::random() * height)
                        .floor()
                        .max(30.0)
                        .min(height - 30.0),
                },
                finished: false,
            })
            .collect(),
        speed: Speed { dx: 0.0, dy: 0.0 },
    };

    let wrapper = GameWrapper {
        game: Arc::new(Mutex::new(Game {
            speed: 2.0,
            context,
            width,
            height,
            task_distance: 32.0,
            kill_distance: 64.0,
            inputs: InputState {
                up: false,
                down: false,
                left: false,
                right: false,
                kill: false,
                activate: false,
                report: false,
            },
            local_player_uuid: Some(my_uuid),
            players: vec![local_player],
            bodies: Vec::new(),
            socket: ws,
        })),
    };

    {
        let game = wrapper
            .game
            .lock()
            .expect("Could not get a lock on the game we just created??");

        let game_clone = wrapper.game.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            // Starting with assuming text messages. Can make efficient later (bson?).
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let strng: String = txt.into();
                let message: Message = match serde_json::from_str(&strng) {
                    Ok(m) => m,
                    Err(e) => {
                        console_log!("Unable to deserialize {:?} – {:?}", strng, e);
                        return;
                    }
                };
                console_log!("message from network: {:?}", message);
                match game_clone
                    .lock()
                    .expect("Internal error: could not get a lock on the game")
                    .handle_msg(message)
                {
                    Ok(()) => (),
                    Err(e) => {
                        console_log!("Error handling message {:?} – {:?}", strng, e);
                    }
                }
            } else {
                console_log!("non-string message received! {:?}", e.data());
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        // set message event handler on WebSocket
        game.socket
            .set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        // forget the callback to keep it alive
        onmessage_callback.forget();

        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            console_log!("error event on websocket: {:?}", e);
        }) as Box<dyn FnMut(ErrorEvent)>);
        game.socket
            .set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let onclose_callback = Closure::wrap(Box::new(move |_| {
            console_log!("websocket closed");
        }) as Box<dyn FnMut(ErrorEvent)>);
        game.socket
            .set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        // TODO: wait on socket to connect before returning.
        let game_clone = wrapper.game.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            console_log!("socket opened");
            let game = game_clone
                .lock()
                .expect("Internal error: could not get a lock on the game");
            game.send_msg(&Message::Join(
                game.local_player()
                    .expect("Internal error: could not get local player during init")
                    .clone(),
            ))
            .expect("Join game message failed to send");
        }) as Box<dyn FnMut(JsValue)>);
        game.socket
            .set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();
    }

    Ok(wrapper)
}

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    Move(MoveMessage),
    Killed(DeadBody),
    FinishedTask(FinishedTask),
    Join(Player),
    Snapshot(Snapshot),
}

#[derive(Serialize, Deserialize, Debug)]
struct MoveMessage {
    color: Color,
    speed: Speed,
    position: Position,
}

#[derive(Serialize, Deserialize, Debug)]
struct FinishedTask {
    color: Color,
    index: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct Snapshot {
    bodies: Vec<DeadBody>,
    players: Vec<Player>,
}
