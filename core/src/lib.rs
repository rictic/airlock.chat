mod game_state;
mod player;
mod protocol;
mod server;
pub use crate::game_state::*;
pub use crate::player::*;
pub use crate::protocol::*;
pub use crate::server::*;
mod tests;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// Set up a console.log macro. This has to live in the main file of the crate
// apparently, for macro-related reasons.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console)]
  pub fn log(s: &str);
}

// If running in wasm, console_log! calls the wasm_bindgen console.log.
#[macro_export]
#[cfg(target_arch = "wasm32")]
macro_rules! console_log {
    ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
}

// If running outside of wasm, console_log! prints to stdout.
#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (println!($($t)*))
}
