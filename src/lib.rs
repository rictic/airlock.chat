mod utils;
use std::error::Error;
use std::f64;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Pink,
    Blue,
    Orange,
    White,
    Black,
}

impl Color {
    fn as_js_value(&self) -> JsValue {
        JsValue::from_str(match self {
            Color::Red => "red",
            Color::Pink => "hotpink",
            Color::Blue => "blue",
            Color::Orange => "orange",
            Color::White => "white",
            Color::Black => "black",
        })
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
pub struct Player {
    color: Color,
    x: f64,
    y: f64,
    dead: bool,
    inputs: InputState,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Game {
    speed: f64,
    width: f64,
    height: f64,
    kill_distance: f64,
    local_player_color: Option<Color>,
    context: web_sys::CanvasRenderingContext2d,
    players: Vec<Player>,
}

// The state of user input at some point in time. i.e. what buttons is
// the user holding down?
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub q: bool,
}

#[wasm_bindgen]
impl Game {
    fn draw_internal(&mut self) -> Result<(), Box<dyn Error>> {
        let context = &self.context;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        context.begin_path();
        context.rect(0.0, 0.0, self.width, self.height);
        context.set_fill_style(&JsValue::from_str("#f3f3f3"));
        context.fill();
        context.stroke();

        // Draw the conference table
        self.circle(275.0, 275.0, 75.0)?;

        for player in self.players.iter() {
            if !player.dead {
                self.draw_player(player)?
            }
        }

        Ok(())
    }

    fn draw_player(&self, player: &Player) -> Result<(), &'static str> {
        self.context.begin_path();
        let radius = 10.0;
        self.context.move_to(player.x + radius, player.y);
        self.context
            .arc(player.x, player.y, radius, 0.0, f64::consts::PI * 2.0)
            .map_err(|_| "Failed to draw a circle.")?;
        self.context.set_fill_style(&player.color.as_js_value());
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
        let local_player_color = match &self.local_player_color {
            None => return Ok(()), // not controlling anything
            Some(c) => *c,
        };

        let mut kill_position: Option<(f64, f64)> = None;

        for player in self.players.iter_mut() {
            if player.color != local_player_color {
                continue;
            }
            let inputs = player.inputs;
            if inputs.up {
                player.y -= self.speed * time_steps_passed
            }
            if inputs.down {
                player.y += self.speed * time_steps_passed
            }
            if inputs.left {
                player.x -= self.speed * time_steps_passed
            }
            if inputs.right {
                player.x += self.speed * time_steps_passed
            }

            if inputs.q {
                kill_position = Some((player.x, player.y));
            }
        }

        if let Some(position) = kill_position {
            self.kill_player_near(position)?
        }

        Ok(())
    }

    fn kill_player_near(&mut self, position: (f64, f64)) -> Result<(), &'static str> {
        let local_player_color = match &self.local_player_color {
            None => return Ok(()), // not controlling anything
            Some(c) => *c,
        };

        let mut killed_player: Option<&mut Player> = None;
        let closest_distance = self.kill_distance;

        for player in self.players.iter_mut() {
            if player.color == local_player_color {
                continue;
            }

            let distance = ((position.0 - player.x).powi(2) + (position.1 - player.y).powi(2))
                .sqrt()
                .abs();

            if distance < closest_distance {
                killed_player = Some(player);
            }
        }

        if let Some(player) = killed_player {
            player.dead = true;
        }

        Ok(())
    }

    // These params describe whether the player is currently holding down each
    // of the buttons.
    pub fn draw(&mut self) -> Option<String> {
        match self.draw_internal() {
            Ok(()) => None,
            Err(e) => Some(format!("Error: {}", e)),
        }
    }

    fn local_player(&mut self) -> Option<&mut Player> {
        let local_player_color = self.local_player_color?;
        for player in self.players.iter_mut() {
            if player.color == local_player_color {
                return Some(player);
            }
        }
        None
    }

    pub fn simulate(&mut self, elapsed: f64) -> Option<String> {
        let result = self.simulate_internal(elapsed);
        match result {
            Ok(()) => None,
            Err(s) => Some(s.to_string()),
        }
    }

    pub fn set_inputs(&mut self, up: bool, down: bool, left: bool, right: bool, q: bool) {
        let player = match self.local_player() {
            None => return,
            Some(p) => p,
        };
        player.inputs = InputState {
            up,
            down,
            left,
            right,
            q,
        };
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
        x: 0.0,
        y: 0.0,
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
pub fn make_game() -> Result<Game, JsValue> {
    start_websocket()?;
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
        player.x = 275.0 + (100.0 * ((i as f64) / num_players * 2.0 * f64::consts::PI).sin());
        player.y = 275.0 + (100.0 * ((i as f64) / num_players * 2.0 * f64::consts::PI).cos());
    }
    let CanvasInfo {
        context,
        width,
        height,
    } = get_canvas_info()
        .map_err(|e| JsValue::from(format!("Error initializing canvas: {}", e)))?;
    Ok(Game {
        speed: 2.0,
        context,
        width,
        height,
        kill_distance: 64.0,
        local_player_color: Some(Color::random()),
        players,
    })
}

pub fn start_websocket() -> Result<(), JsValue> {
    let ws = WebSocket::new("ws://localhost:3012")?;
    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        // Starting with assuming text messages. Can make efficient later (bson?).
        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            console_log!("message from network: {:?}", txt);
        } else {
            console_log!("non-string message received! {:?}", e.data());
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    // set message event handler on WebSocket
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    // forget the callback to keep it alive
    onmessage_callback.forget();

    let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
        console_log!("error event on websocket: {:?}", e);
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    let onclose_callback = Closure::wrap(Box::new(move |_| {
        console_log!("websocket closed");
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
    onclose_callback.forget();

    let cloned_ws = ws.clone();
    let onopen_callback = Closure::wrap(Box::new(move |_| {
        console_log!("socket opened");
        match cloned_ws.send_with_str("ping") {
            Ok(_) => (),
            Err(err) => console_log!("error sending message: {:?}", err),
        }
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    Ok(())
}
