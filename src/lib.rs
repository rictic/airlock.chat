mod utils;
use std::error::Error;
use std::f64;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

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

#[derive(Clone)]
pub struct Player {
    color: Color,
    x: f64,
    y: f64,
    dead: bool,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Game {
    speed: f64,
    width: f64,
    height: f64,
    kill_distance: f64,
    local_player_color: Option<Color>,
    last_input: InputState,
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

    fn simulate_internal(&mut self, elapsed: f64, inputs: InputState) -> Result<(), &'static str> {
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

            if !inputs.q && self.last_input.q {
                kill_position = Some((player.x, player.y));
            }
        }

        match kill_position {
            Some(position) => self.kill_player_near(position)?,
            None => return Ok(()),
        }

        Ok(())
    }

    fn kill_player_near(&mut self, position: (f64, f64)) -> Result<(), &'static str> {
        let local_player_color = match &self.local_player_color {
            None => return Ok(()), // not controlling anything
            Some(c) => *c,
        };

        for player in self.players.iter_mut() {
            if player.color == local_player_color {
                continue;
            }

            let distance = ((position.0 - player.x).powi(2) + (position.1 - player.y).powi(2))
                .sqrt()
                .abs();

            if distance < self.kill_distance {
                player.dead = true;
                break;
            }
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

    pub fn simulate(
        &mut self,
        elapsed: f64,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        q: bool,
    ) -> Option<String> {
        let next_input = InputState {
            up,
            down,
            left,
            right,
            q,
        };

        let result = self.simulate_internal(elapsed, next_input);

        self.last_input = next_input;

        match result {
            Ok(()) => None,
            Err(s) => Some(s.to_string()),
        }
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
    }
}

#[wasm_bindgen]
pub fn make_game() -> Result<Game, JsValue> {
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
        last_input: InputState {
            up: false,
            down: false,
            left: false,
            right: false,
            q: false,
        },
        players,
    })
}
