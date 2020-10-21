mod canvas;
mod js_api;
mod network;
mod utils;
pub use crate::js_api::*;
use std::f64;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// Set up a console.log macro. This has to live in the main file of the crate
// it seems, for macro-related reasons.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
}
