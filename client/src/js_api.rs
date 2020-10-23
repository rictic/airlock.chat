use crate::canvas::*;
use crate::network::create_websocket_and_listen;
use instant::Instant;
use rust_us_core::*;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
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
  previous_frame_time: Instant,
  game: Arc<Mutex<Option<GameAsPlayer>>>,
  playback_server: Option<PlaybackServer>,
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
    skip_back: bool,
    skip_forward: bool,
    pause_playback: bool,
  ) -> Result<(), JsValue> {
    let mut game = self
      .game
      .lock()
      .expect("Internal Error: could not get a lock on the game");
    if game.is_none() {
      return Ok(());
    }
    let game = game.as_mut().unwrap();
    let prev_input = game.inputs();
    let input = InputState {
      up,
      down,
      left,
      right,
      kill,
      report,
      activate,
      play,
      skip_back,
      skip_forward,
      pause_playback,
    };
    if let Some(playback_server) = &mut self.playback_server {
      if input.skip_back && !prev_input.skip_back {
        let time = playback_server.current_time();
        playback_server
          .skip_to(
            time
              .checked_sub(Duration::from_secs(5))
              .unwrap_or_else(|| Duration::from_secs(0)),
            game,
          )
          .map_err(|e| JsValue::from(format!("{}", e)))?;
        console_log!("Skipped back");
      } else if input.skip_forward && !prev_input.skip_forward {
        let time = playback_server.current_time();
        playback_server
          .skip_to(
            time
              .checked_add(Duration::from_secs(5))
              .unwrap_or_else(|| playback_server.duration()),
            game,
          )
          .map_err(|e| JsValue::from(format!("{}", e)))?;
      } else if input.pause_playback && !prev_input.pause_playback {
        playback_server.toggle_pause();
        if !playback_server.paused() {
          self.previous_frame_time = Instant::now();
        }
      }
    }
    if game.state.status.finished() {
      return Ok(());
    }
    game.take_input(input).map_err(JsValue::from)
  }

  pub fn simulate(&mut self) -> Result<bool, JsValue> {
    let mut game = self
      .game
      .lock()
      .expect("Internal Error: could not get a lock on the game");
    if game.is_none() {
      return Ok(false);
    }
    let game = game.as_mut().unwrap();
    let now = Instant::now();
    let elapsed = now - self.previous_frame_time;
    self.previous_frame_time = now;
    if let Some(playback_server) = &mut self.playback_server {
      if playback_server.paused() {
        // Skip all simulation and drawing while paused until we
        // get the next input.
        return Ok(true);
      }
      playback_server
        .simulate(elapsed, game, false)
        .map_err(|e| JsValue::from(format!("{}", e)))?;
    }
    if game.state.status == GameStatus::Connecting {
      return Ok(false);
    }
    Ok(game.simulate(elapsed))
  }

  pub fn draw(&mut self) -> Result<(), JsValue> {
    self.canvas.draw(self.game.clone())
  }

  pub fn get_status(&self) -> String {
    let game = self.game.lock().unwrap();
    if game.is_none() {
      return "Connecting to game...".to_string();
    }
    let game = game.as_ref().unwrap();
    let local_player = game.local_player();
    if let Some(playback) = &self.playback_server {
      let paused = playback.paused();
      let finished = game.state.status.finished();
      let prefix = match (finished, paused) {
        (true, _) => "Replay finished at",
        (false, true) => "Replay paused at",
        (false, false) => "Watching replay at",
      };
      return format!(
        "{} {} of {}\n{}",
        prefix,
        format_time(playback.current_time()),
        format_time(playback.duration()),
        self.get_status_internal(&game.state.status, local_player)
      );
    }
    self.get_status_internal(&game.state.status, local_player)
  }

  fn get_status_internal(&self, status: &GameStatus, local_player: Option<&Player>) -> String {
    match status {
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
      GameStatus::Playing(PlayState::Night) => {
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
      GameStatus::Playing(PlayState::Day(_)) => "".to_string(),
    }
  }
}

fn format_time(duration: Duration) -> String {
  let secs = duration.as_secs();
  let minutes = secs / 60;
  let secs = secs % 60;
  format!("{}:{:02}", minutes, secs)
}

fn get_recorded_game() -> Result<Option<RecordedGame>, JsValue> {
  let local_storage = web_sys::window()
    .ok_or("no window")?
    .local_storage()?
    .ok_or("no window.localStorage")?;
  let value = local_storage.get("latest game")?;
  let encoded_game = match value {
    None => return Ok(None),
    Some(g) => g,
  };
  let game = serde_json::from_str(&encoded_game).map_err(|e| {
    format!(
      "Unable to decode game recording from localStorage {:?} – {:?}",
      encoded_game, e
    )
  })?;
  let game = match game {
    ServerToClientMessage::Replay(game) => game,
    _ => {
      return Err(
        format!(
          "Could not decode recorded game from local storage. Expected a Replay but found a {}",
          game.kind()
        )
        .into(),
      )
    }
  };
  Ok(Some(game))
}

pub fn save_recorded_game(encoded_game: &str) -> Result<(), JsValue> {
  let local_storage = web_sys::window()
    .ok_or("no window")?
    .local_storage()?
    .ok_or("no window.localStorage")?;
  local_storage.set("latest game", encoded_game)?;
  Ok(())
}

#[wasm_bindgen]
pub fn make_game(name: String) -> Result<GameWrapper, JsValue> {
  crate::utils::set_panic_hook();
  let location = web_sys::window().ok_or("no window")?.location();
  let should_playback = location.search()?.contains("recording");
  let wrapper;
  if !should_playback {
    wrapper = GameWrapper {
      previous_frame_time: Instant::now(),
      canvas: Canvas::find_in_document()?,
      game: Arc::new(Mutex::new(None)),
      playback_server: None,
    };
    let join = JoinRequest::JoinAsPlayer {
      name,
      preferred_color: Color::random(),
    };
    create_websocket_and_listen(wrapper.game.clone(), join)?;
  } else {
    let recording = match get_recorded_game()? {
      None => return Err(JsValue::from("No saved game found")),
      Some(recording) => recording,
    };
    console_log!(
      "Starting replay of version {} inside game with version {}",
      recording.version,
      get_version_sha()
    );
    let playback_server = Some(PlaybackServer::new(recording));
    let connection = Box::new(PlaybackTx {});
    let mut game_as_player = GameAsPlayer::new(UUID::random(), connection);
    game_as_player.state.status = GameStatus::Lobby;
    wrapper = GameWrapper {
      previous_frame_time: Instant::now(),
      canvas: Canvas::find_in_document()?,
      playback_server,
      game: Arc::new(Mutex::new(Some(game_as_player))),
    }
  }

  Ok(wrapper)
}
