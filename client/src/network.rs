use crate::*;
use rust_us_core::ClientToServerMessage;
use rust_us_core::GameAsPlayer;
use rust_us_core::GameTx;
use rust_us_core::ServerToClientMessage;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

#[derive(Clone)]
pub struct WebSocketTx {
  socket: WebSocket,
}

impl WebSocketTx {
  pub fn new(ws: WebSocket) -> WebSocketTx {
    WebSocketTx { socket: ws }
  }
}

impl GameTx for WebSocketTx {
  fn send(&self, message: &ClientToServerMessage) -> Result<(), String> {
    let encoded = serde_json::to_string(&message)
      .map_err(|_| JsValue::from_str("Unable to encode Message to json"))
      .map_err(|e| format!("{:?}", e))?;
    self
      .socket
      .send_with_str(&encoded)
      .map_err(|e| format!("{:?}", e))?;
    Ok(())
  }
}

pub fn wire_up_websocket(
  wrapper_wrapper: Arc<Mutex<Option<Arc<Mutex<GameAsPlayer>>>>>,
  ws: &WebSocket,
) {
  let wrapper_wrapper_clone = wrapper_wrapper.clone();
  let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
    // Starting with assuming text messages. Can make efficient later (bson?).
    if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
      let strng: String = txt.into();
      let message: ServerToClientMessage = match serde_json::from_str(&strng) {
        Ok(m) => m,
        Err(e) => {
          console_log!("Unable to deserialize {:?} – {:?}", strng, e);
          return;
        }
      };
      let option_wrapped = &wrapper_wrapper_clone.lock().unwrap();
      let wrapper = option_wrapped.as_ref().unwrap();
      let mut game = wrapper.lock().unwrap();
      match game.handle_msg(message) {
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
  ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
  // forget the callback to keep it alive
  onmessage_callback.forget();

  let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
    console_log!("error event on websocket: {:?}", e);
  }) as Box<dyn FnMut(ErrorEvent)>);
  ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
  onerror_callback.forget();

  let wrapper_wrapper_clone = wrapper_wrapper.clone();
  let onclose_callback = Closure::wrap(Box::new(move |_| {
    console_log!("websocket closed");
    let option_wrapped = &wrapper_wrapper_clone.lock().unwrap();
    let wrapper = option_wrapped.as_ref().unwrap();
    let mut game = wrapper.lock().unwrap();
    game
      .disconnected()
      .expect("Game failed to handle disconnection");
  }) as Box<dyn FnMut(ErrorEvent)>);
  ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
  onclose_callback.forget();

  // TODO: wait on socket to connect before returning.
  let onopen_callback = Closure::wrap(Box::new(move |_| {
    console_log!("socket opened");
    let option_wrapped = &wrapper_wrapper.lock().unwrap();
    let wrapper = &option_wrapped.as_ref().unwrap();
    let mut game = wrapper.lock().unwrap();
    game.connected().expect("Could not handle game.connected()");
  }) as Box<dyn FnMut(JsValue)>);
  ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
  onopen_callback.forget();
}
