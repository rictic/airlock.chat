use crate::canvas::*;
use crate::network::create_websocket_and_listen;
use rust_us_core::*;
use std::f64;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Exposes a JS API for a Canvas and a GameAsPlayer.
#[wasm_bindgen]
pub struct GameWrapper {
  canvas: Canvas,
  game: Arc<Mutex<GameAsPlayer>>,
}

#[wasm_bindgen]
impl GameWrapper {
  #[allow(clippy::too_many_arguments)]
  pub fn set_inputs(
    &mut self,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    kill: bool,
    report: bool,
    activate: bool,
    play: bool,
  ) -> Result<(), JsValue> {
    let mut game = self
      .game
      .lock()
      .expect("Internal Error: could not get a lock on the game");
    if game.state.status.finished() {
      return Ok(());
    }
    game
      .take_input(InputState {
        up,
        down,
        left,
        right,
        kill,
        report,
        activate,
        play,
      })
      .map_err(JsValue::from)
  }

  pub fn simulate(&mut self, elapsed: f64) -> Result<bool, JsValue> {
    let mut game = self
      .game
      .lock()
      .expect("Internal Error: could not get a lock on the game");
    if game.state.status == GameStatus::Connecting {
      return Ok(false);
    }
    Ok(game.state.simulate(elapsed))
  }

  pub fn draw(&mut self) -> Result<(), JsValue> {
    self
      .canvas
      .draw(self.game.clone())
      .map_err(|e| JsValue::from(format!("Error drawing: {}", e)))
  }

  pub fn get_status(&self) -> String {
    let game = self.game.lock().unwrap();
    let local_player = game.local_player();
    match game.state.status {
      GameStatus::Connecting => "Conecting to game...".to_string(),
      GameStatus::Disconnected => "Disconnected from server.".to_string(),
      GameStatus::Lobby => {
        if let Some(local_player) = local_player {
          format!(
            "In the lobby. You're {:?}. Press P to start the game once enough players have joined!",
            local_player.color
          )
        } else {
          "In the lobby. The game is full so you're spectating.".to_string()
        }
      }
      GameStatus::Won(team) => format!("{:?} win!", team),
      GameStatus::Playing(_) => {
        if let Some(local_player) = local_player {
          if local_player.dead {
            if local_player.impostor {
              "You're dead as an impostor! Nothing to do right now but watch.".to_string()
            } else {
              "You're dead as a crewmate! Be sure to finish your tasks.".to_string()
            }
          } else if local_player.impostor {
            "You're an impostor! Kill players by getting near them and pressing Q.".to_string()
          } else {
            "You're a good crewmate! Go to your tasks and press E to solve them, but watch out for impostors trying to kill you!".to_string()
          }
        } else {
          "The game has begun! You're spectating.".to_string()
        }
      }
    }
  }
}

#[wasm_bindgen]
pub fn make_game(name: String) -> Result<GameWrapper, JsValue> {
  crate::utils::set_panic_hook();
  // Ok, this is pretty crazy, but I can explain.
  // We need to set up the websocket callbacks using wasm_bindgen for the initial connection,
  // and handling messages, and disconnecting.
  // All of the actual logic happens inside of Game.
  // The callbacks need to access the Game.
  // Once we hand the WebSocket to the Game then it owns it, so we have to do our websocket setup
  // before creating the game, but we need to access the game inside the callbacks...
  // So here we are.
  let wrapper_wrapper: Arc<Mutex<Option<Arc<Mutex<GameAsPlayer>>>>> = Arc::new(Mutex::new(None));

  let my_uuid = UUID::random();
  let join = Join {
    uuid: my_uuid,
    name,
    preferred_color: Color::random(),
  };
  let game_tx = create_websocket_and_listen(wrapper_wrapper.clone(), join)?;
  let wrapper = GameWrapper {
    canvas: Canvas::find_in_document()?,
    game: Arc::new(Mutex::new(GameAsPlayer::new(my_uuid, game_tx))),
  };
  {
    let mut wrapped = wrapper_wrapper.lock().unwrap();
    *wrapped = Some(wrapper.game.clone());
  }
  Ok(wrapper)
}
