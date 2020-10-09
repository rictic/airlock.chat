mod canvas;
mod js_api;
mod network;
mod utils;
pub use crate::js_api::*;
use std::f64;
use wasm_bindgen::prelude::*;

// Set up a console.log macro. This has to live in the main file of the crate
// it seems, for macro-related reasons.

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}
