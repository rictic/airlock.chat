mod utils;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Color {
    Red,
    Pink,
    Blue,
    Orange,
    White,
    Black,
}

impl Color {
    fn to_str(&self) -> &'static str {
        match self {
            Color::Red => "#ff0102",
            Color::Pink => "#ff69b4",
            Color::Blue => "#1601ff",
            Color::Orange => "#ffa502",
            Color::White => "#ffffff",
            Color::Black => "#000000",
        }
    }

    fn as_js_value(&self) -> JsValue {
        JsValue::from_str(self.to_str())
    }

    fn as_semitransparant_js_value(&self) -> JsValue {
        JsValue::from(format!("{}88", self.to_str()))
    }

    fn random() -> Color {
        let rand = js_sys::Math::random();
        match (rand * 6.0).floor() as u32 {
            0 => Color::Red,
            1 => Color::Pink,
            2 => Color::Blue,
            3 => Color::Orange,
            4 => Color::White,
            _ => Color::Black,
        }
    }
}

// impl rand::distributions::Distribution<Color> {}

#[derive(Clone, Copy)]
struct Player {
    color: Color,
    position: Position,
    dead: bool,
    inputs: InputState,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Position {
    x: f64,
    y: f64,
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
    local_player_color: Option<Color>,
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

// The state of user input at some point in time. i.e. what buttons is
// the user holding down?
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub q: bool,
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
        self.circle(275.0, 275.0, 75.0)?;

        let show_dead_people = match self.local_player() {
            None => true,
            Some(p) => p.dead,
        };

        for player in self.players.iter() {
            if show_dead_people || !player.dead {
                self.draw_player(player)?
            }
        }
        for body in self.bodies.iter() {
            self.draw_body(*body)?;
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
        self.context.fill();
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
            let inputs = player.inputs;
            if inputs.up {
                player.position.y -= self.speed * time_steps_passed
            }
            if inputs.down {
                player.position.y += self.speed * time_steps_passed
            }
            if inputs.left {
                player.position.x -= self.speed * time_steps_passed
            }
            if inputs.right {
                player.position.x += self.speed * time_steps_passed
            }

            // We don't handle inputs.q here because player position may be
            // out of sync, but we _super_ don't want to let life or death
            // get out of sync.
        }

        Ok(())
    }

    fn kill_player_near(&mut self, position: Position) -> Result<Option<&Player>, &'static str> {
        let local_player_color = match &self.local_player_color {
            None => return Ok(None), // not controlling anything
            Some(c) => *c,
        };

        let mut killed_player: Option<&mut Player> = None;
        let mut closest_distance = self.kill_distance;

        for player in self.players.iter_mut() {
            if player.color == local_player_color || player.dead {
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
            return Ok(Some(player));
        }

        Ok(None)
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
        let local_player_color = self.local_player_color?;
        for player in self.players.iter() {
            if player.color == local_player_color {
                return Some(player);
            }
        }
        None
    }

    fn local_player_mut(&mut self) -> Option<&mut Player> {
        let local_player_color = self.local_player_color?;
        for player in self.players.iter_mut() {
            if player.color == local_player_color {
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

    fn set_inputs(&mut self, new_input: InputState) -> Result<(), JsValue> {
        let player = match self.local_player() {
            None => return Ok(()),
            Some(p) => p,
        };
        if new_input == player.inputs {
            return Ok(()); // quick exit for the boring case
        }
        // Read the parts of the local player that we care about.
        let is_killing = !player.inputs.q && new_input.q;
        let position = player.position;
        // ok, we're done touching player at this point. we redeclare it
        // below so we can use it again, next time mutably.

        // Now that we don't reference player any longer, the borrow checker
        // is ok with us mutating self.
        let mut kill_info: Option<DeadBody> = None;
        if is_killing {
            // local player just hit the q button
            let dead_player = self.kill_player_near(position).map_err(JsValue::from)?;
            if let Some(dead_player) = dead_player {
                kill_info = Some(DeadBody {
                    position: dead_player.position,
                    color: dead_player.color,
                });
            }
            // this is also a good time to broadcast the kill
        }
        // Move the killer on top of the new body.
        {
            let player = self
                .local_player_mut()
                .ok_or_else(|| JsValue::from("Did player kill themselves?"))?;
            player.inputs = new_input;
            if let Some(DeadBody { position, color: _ }) = kill_info {
                player.position = position;
            }
        }
        if let Some(body) = kill_info {
            self.send_msg(&Message::Killed(body))?;
            self.bodies.push(body);
        }

        let player: &Player = self.local_player().unwrap();
        self.send_msg(&Message::Move(MoveMessage {
            color: player.color,
            inputs: player.inputs,
            current_position: player.position,
        }))?;
        Ok(())
    }

    fn send_msg(&self, message: &Message) -> Result<(), JsValue> {
        let encoded = serde_json::to_string(message)
            .map_err(|_| JsValue::from_str("Unable to encode Message to json"))?;
        self.socket.send_with_str(&encoded)?;
        Ok(())
    }

    fn handle_msg(&mut self, message: &Message) -> Result<(), JsValue> {
        match message {
            Message::Killed(body) => {
                for player in self.players.iter_mut() {
                    if player.color == body.color {
                        player.dead = true;
                    }
                }
                self.bodies.push(*body);
            }
            Message::Move(moved) => {
                for player in self.players.iter_mut() {
                    if player.color == moved.color {
                        player.inputs = moved.inputs;
                        player.position = moved.current_position;
                    }
                }
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

fn make_player(color: Color) -> Player {
    Player {
        color,
        dead: false,
        position: Position { x: 0.0, y: 0.0 },
        inputs: InputState {
            up: false,
            down: false,
            left: false,
            right: false,
            q: false,
        },
    }
}

#[wasm_bindgen]
pub struct GameWrapper {
    game: Arc<Mutex<Game>>,
}

#[wasm_bindgen]
impl GameWrapper {
    pub fn set_inputs(
        &mut self,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        q: bool,
    ) -> Result<(), JsValue> {
        let mut game = self
            .game
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        game.set_inputs(InputState {
            up,
            down,
            left,
            right,
            q,
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

#[wasm_bindgen]
pub fn make_game() -> Result<GameWrapper, JsValue> {
    let mut players = vec![
        make_player(Color::Red),
        make_player(Color::Pink),
        make_player(Color::Blue),
        make_player(Color::Orange),
        make_player(Color::White),
        make_player(Color::Black),
    ];
    let num_players = players.len() as f64;
    for (i, player) in players.iter_mut().enumerate() {
        // Place the players equidistant around the meeting table.
        player.position = Position {
            x: 275.0 + (100.0 * ((i as f64) / num_players * 2.0 * f64::consts::PI).sin()),
            y: 275.0 + (100.0 * ((i as f64) / num_players * 2.0 * f64::consts::PI).cos()),
        };
    }
    let CanvasInfo {
        context,
        width,
        height,
    } = get_canvas_info()
        .map_err(|e| JsValue::from(format!("Error initializing canvas: {}", e)))?;
    let ws = WebSocket::new("ws://localhost:3012")?;
    let wrapper = GameWrapper {
        game: Arc::new(Mutex::new(Game {
            speed: 2.0,
            context,
            width,
            height,
            kill_distance: 64.0,
            local_player_color: Some(Color::random()),
            players,
            bodies: vec![DeadBody {
                color: Color::Pink,
                position: Position { x: 50.0, y: 50.0 },
            }],
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
                    .handle_msg(&message)
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

        let cloned_ws = game.socket.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            console_log!("socket opened");
            match cloned_ws.send_with_str("ping") {
                Ok(_) => (),
                Err(err) => console_log!("error sending message: {:?}", err),
            }
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
}

#[derive(Serialize, Deserialize, Debug)]
struct MoveMessage {
    color: Color,
    inputs: InputState,
    current_position: Position,
}
