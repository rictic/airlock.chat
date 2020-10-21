use crate::js_api::save_recorded_game;
use rust_us_core::console_log;
use rust_us_core::get_version_sha;
use rust_us_core::ClientToServerMessage;
use rust_us_core::GameAsPlayer;
use rust_us_core::GameTx;
use rust_us_core::JoinRequest;
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

fn get_websocket_url() -> Result<String, JsValue> {
  let location = web_sys::window().ok_or("no window")?.location();
  let scheme = location.protocol()?;
  let port = location.port()?;
  let hostname = location.hostname()?;

  if scheme == "https:" {
    // we're in prod, just use the prod URL
    return Ok(format!("wss://{}/", hostname));
  }

  if port == "8000" {
    // we're in dev mode, use the dev mode port
    return Ok(format!("ws://{}:3012/", hostname));
  }

  // we're running the prod server locally without TLS
  Ok(format!("ws://{}/", hostname))
}

// Creates a websocket and hooks it up to the callbacks on the given GameAsPlayer.
pub fn create_websocket_and_listen(
  game_as_player: Arc<Mutex<Option<GameAsPlayer>>>,
  join: JoinRequest,
) -> Result<(), JsValue> {
  let ws = WebSocket::new(&get_websocket_url()?)?;

  let game_as_player_clone = game_as_player.clone();
  let ws_clone = ws.clone();
  let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
    // Starting with assuming text messages. Can make efficient later (bson?).
    let message: ServerToClientMessage;
    if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
      let strng: String = txt.into();
      message = match serde_json::from_str(&strng) {
        Ok(m) => m,
        Err(e) => {
          console_log!("Unable to deserialize {:?} – {:?}", strng, e);
          return;
        }
      };
      if let ServerToClientMessage::Replay(_) = &message {
        match save_recorded_game(&strng) {
          Ok(()) => console_log!("Saved replay!"),
          Err(e) => console_log!("Error saving game recording to localStorage: {:?}", e),
        }
      }
    } else {
      console_log!("Unable to handle binary encoded message.");
      return;
    }
    if let ServerToClientMessage::Welcome {
      connection_id,
      game_id,
    } = message
    {
      let clone = &game_as_player_clone.clone();
      let mut wrapped = clone.lock().unwrap();
      *wrapped = Some(GameAsPlayer::new(
        connection_id,
        game_id,
        Box::new(WebSocketTx::new(ws_clone.clone())),
      ));
    }
    let clone = &game_as_player_clone.clone();
    let mut option_wrapped = clone.lock().unwrap();
    let game_as_player = match option_wrapped.as_mut() {
      // Got a non-welcome message before our welcome message.
      None => return,
      Some(g) => g,
    };
    match game_as_player.handle_msg(message) {
      Ok(()) => (),
      Err(e) => {
        console_log!("Player failed to handle message: {:?}", e);
      }
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
    let mut option_wrapped = game_as_player.lock().unwrap();
    let game_as_player = option_wrapped.as_mut().unwrap();
    game_as_player
      .disconnected()
      .expect("Game failed to handle disconnection");
  }) as Box<dyn FnMut(ErrorEvent)>);
  ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
  onclose_callback.forget();

  let ws_clone = ws.clone();
  let onopen_callback = Closure::wrap(Box::new(move |_| {
    console_log!("socket opened");
    let tx = Box::new(WebSocketTx::new(ws_clone.clone()));
    tx.send(&ClientToServerMessage::Join {
      version: get_version_sha().to_string(),
      details: join.clone(),
    })
    .unwrap();
  }) as Box<dyn FnMut(JsValue)>);
  ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
  onopen_callback.forget();
  Ok(())
}
