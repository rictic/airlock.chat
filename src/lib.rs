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

#[wasm_bindgen]
#[derive(Clone)]
pub struct Game {
    x: f64,
    y: f64,
    speed: f64,
    width: f64,
    height: f64,
    context: web_sys::CanvasRenderingContext2d,
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
}

#[wasm_bindgen]
impl Game {
    fn draw_internal(&mut self) -> Result<(), Box<dyn Error>> {
        let context = &self.context;
        context.clear_rect(0.0, 0.0, self.width, self.height);

        context.begin_path();

        // Draw the outer circle.
        context
            .arc(75.0, 75.0, 50.0, 0.0, f64::consts::PI * 2.0)
            .map_err(|_| "Failed to draw an arc.")?;

        // Draw the mouth.
        context.move_to(110.0, 75.0);
        context
            .arc(75.0, 75.0, 35.0, 0.0, f64::consts::PI)
            .map_err(|_| "Failed to draw an arc.")?;

        // Draw the left eye.
        context.move_to(65.0 + self.x, 65.0 + self.y);
        context
            .arc(
                60.0 + self.x,
                65.0 + self.y,
                5.0,
                0.0,
                f64::consts::PI * 2.0,
            )
            .map_err(|_| "Failed to draw an arc.")?;

        // Draw the right eye.
        context.move_to(95.0, 65.0);
        context
            .arc(90.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
            .map_err(|_| "Failed to draw an arc.")?;

        context.stroke();

        Ok(())
    }

    fn simulate_internal(&mut self, elapsed: f64, inputs: InputState) {
        // elapsed is the time, in milliseconds, that has passed since the
        // last time we simulated.
        // By making our simulations relative to the amount of time that's passed,
        // the game will progress the same regardless of the frame rate, which may
        // vary between 30fps and 144fps even if our performance is perfect!
        let time_steps_passed = elapsed / 16.0;
        if inputs.up {
            self.y -= self.speed * time_steps_passed
        }
        if inputs.down {
            self.y += self.speed * time_steps_passed
        }
        if inputs.left {
            self.x -= self.speed * time_steps_passed
        }
        if inputs.right {
            self.x += self.speed * time_steps_passed
        }
    }

    // These params describe whether the player is currently holding down each
    // of the buttons.
    pub fn draw(&mut self) -> Option<String> {
        match self.draw_internal() {
            Ok(()) => None,
            Err(e) => Some(format!("Error: {}", e)),
        }
    }

    pub fn simulate(&mut self, elapsed: f64, up: bool, down: bool, left: bool, right: bool) {
        self.simulate_internal(
            elapsed,
            InputState {
                up,
                down,
                left,
                right,
            },
        );
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

// This is a poor substitute for a Result<Game, String> doesn't want
// to be passed from wasm to JS for some reason.
#[wasm_bindgen]
pub struct MakeGameResult {
    game: Option<Game>,
    error: Option<String>,
}

#[wasm_bindgen]
impl MakeGameResult {
    pub fn get_game(&self) -> Option<Game> {
        self.game.clone()
    }

    pub fn get_error(&self) -> Option<String> {
        self.error.clone()
    }
}

#[wasm_bindgen]
pub fn make_game() -> MakeGameResult {
    match get_canvas_info() {
        Ok(CanvasInfo {
            context,
            width,
            height,
        }) => MakeGameResult {
            game: Some(Game {
                x: 0.0,
                y: 0.0,
                speed: 2.0,
                context,
                width,
                height,
            }),
            error: None,
        },
        Err(e) => MakeGameResult {
            game: None,
            error: Some(format!("Error: {}", e)),
        },
    }
}
