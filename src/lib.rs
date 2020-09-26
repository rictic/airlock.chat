mod utils;
use std::f64;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct Game {
    x: f64,
    y: f64,
}

#[wasm_bindgen]
impl Game {
    // These params describe whether the player is currently holding down each
    // of the buttons.
    pub fn draw(&mut self, up: bool, down: bool, left: bool, right: bool) -> Option<bool> {
        let document = web_sys::window()?.document()?;
        let canvas = document.get_element_by_id("canvas")?;
        let canvas: web_sys::HtmlCanvasElement =
            canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok()?;

        let context = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .ok()?;

        context.clear_rect(0.0, 0.0, canvas.width().into(), canvas.height().into());

        if up {
            self.y -= 1.0
        }
        if down {
            self.y += 1.0
        }
        if left {
            self.x -= 1.0
        }
        if right {
            self.x += 1.0
        }

        context.begin_path();

        // Draw the outer circle.
        context
            .arc(75.0, 75.0, 50.0, 0.0, f64::consts::PI * 2.0)
            .ok()?;

        // Draw the mouth.
        context.move_to(110.0, 75.0);
        context.arc(75.0, 75.0, 35.0, 0.0, f64::consts::PI).ok()?;

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
            .ok()?;

        // Draw the right eye.
        context.move_to(95.0, 65.0);
        context
            .arc(90.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
            .ok()?;

        context.stroke();

        Some(true)
    }
}

#[wasm_bindgen]
pub fn make_game() -> Game {
    Game { x: 0.0, y: 0.0 }
}
