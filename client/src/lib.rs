mod utils;
use rust_us_core::*;
use std::error::Error;
use std::f64;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

struct Canvas {
    width: f64,
    height: f64,
    context: web_sys::CanvasRenderingContext2d,
}

impl Canvas {
    fn draw(&self, game: &Game) -> Option<String> {
        match self.draw_internal(game) {
            Ok(()) => None,
            Err(e) => Some(format!("Error: {}", e)),
        }
    }

    fn draw_internal(&self, game: &Game) -> Result<(), Box<dyn Error>> {
        let context = &self.context;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        context.begin_path();
        context.rect(0.0, 0.0, self.width, self.height);
        context.set_fill_style(&JsValue::from_str("#f3f3f3"));
        context.fill();
        context.stroke();

        // Draw the conference table
        context.set_stroke_style(&JsValue::from_str("#000000"));
        context.set_fill_style(&JsValue::from_str("#358"));
        self.circle(275.0, 275.0, 75.0)?;

        let show_dead_people = match game.local_player() {
            None => true,
            Some(p) => p.dead || p.impostor,
        };

        // Draw tasks, then bodies, then players on top, so tasks are behind everything, then
        // bodies, then imps. That way imps can stand on top of bodies.
        // However maybe we should instead draw items from highest to lowest, vertically?
        if let Some(local_player) = game.local_player() {
            if game.status == GameStatus::Playing {
                for task in local_player.tasks.iter() {
                    if task.finished {
                        continue;
                    }
                    self.draw_task(*task, local_player.impostor)?;
                }
            }
        }
        for body in game.bodies.iter() {
            self.draw_body(*body)?;
        }
        for player in game.players.iter() {
            if show_dead_people || !player.dead {
                self.draw_player(player)?
            }
        }

        Ok(())
    }

    fn draw_player(&self, player: &Player) -> Result<(), &'static str> {
        self.context.begin_path();
        let radius = 10.0;
        self.context
            .move_to(player.position.x + radius, player.position.y);
        self.context
            .arc(
                player.position.x,
                player.position.y,
                radius,
                0.0,
                f64::consts::PI * 2.0,
            )
            .map_err(|_| "Failed to draw a circle.")?;
        let color = if player.dead {
            JsValue::from(format!("{}88", player.color.to_str()))
        } else {
            JsValue::from_str(player.color.to_str())
        };
        self.context.set_fill_style(&color);
        let stroke_color = if player.dead {
            JsValue::from("#00000088")
        } else {
            JsValue::from("#000000")
        };
        self.context.set_stroke_style(&stroke_color);
        self.context.fill();
        self.context.stroke();
        Ok(())
    }

    fn draw_body(&self, body: DeadBody) -> Result<(), &'static str> {
        self.context.begin_path();
        let radius = 10.0;
        self.context
            .move_to(body.position.x + radius, body.position.y);
        self.context
            .arc(
                body.position.x,
                body.position.y,
                radius,
                0.0,
                f64::consts::PI * 1.0,
            )
            .map_err(|_| "Failed to draw a circle.")?;
        self.context
            .set_fill_style(&JsValue::from_str(body.color.to_str()));
        self.context.set_stroke_style(&JsValue::from("#000000"));
        self.context.fill();
        self.context.stroke();
        Ok(())
    }

    fn draw_task(&self, task: Task, fake: bool) -> Result<(), &'static str> {
        self.context.begin_path();
        let len: f64 = 15.0;
        let pos = task.position;
        // drawing an equilateral triangle...
        let height = (len.powf(2.0) - (len / 2.0).powf(2.0)).sqrt();
        self.context.move_to(pos.x + (len / 2.0), pos.y);
        self.context.line_to(pos.x, pos.y + height);
        self.context.line_to(pos.x + len, pos.y + height);
        self.context.line_to(pos.x + (len / 2.0), pos.y);
        if fake {
            self.context.set_fill_style(&JsValue::from("#ffa50244"));
            self.context.set_stroke_style(&JsValue::from("#00000044"));
        } else {
            self.context.set_fill_style(&JsValue::from("#ffa502"));
            self.context.set_stroke_style(&JsValue::from("#000000"));
        }
        self.context.fill();
        self.context.stroke();
        self.context.move_to(pos.x + (len / 2.0), pos.y + 3.0);
        self.context.line_to(pos.x + (len / 2.0), pos.y + 9.0);
        self.context.move_to(pos.x + (len / 2.0), pos.y + 10.0);
        self.context.line_to(pos.x + (len / 2.0), pos.y + 12.0);
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
        self.context.fill();
        Ok(())
    }
}

#[derive(Clone)]
struct WebSocketTx {
    socket: WebSocket,
}

impl GameTx for WebSocketTx {
    fn send(&self, message: &Message) -> Result<(), String> {
        let encoded = serde_json::to_string(&message)
            .map_err(|_| JsValue::from_str("Unable to encode Message to json"))
            .map_err(|e| format!("{:?}", e))?;
        self.socket
            .send_with_str(&encoded)
            .map_err(|e| format!("{:?}", e))?;
        Ok(())
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

#[wasm_bindgen]
pub struct GameWrapper {
    environment: Arc<Mutex<GameEnvironment>>,
}

struct GameEnvironment {
    game: Game,
    canvas: Canvas,
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
        let mut environment = self
            .environment
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        if environment.game.status.finished() {
            return Ok(());
        }
        environment
            .game
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

    pub fn simulate(&mut self, elapsed: f64) -> Result<Option<String>, JsValue> {
        let mut environment = self
            .environment
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        if environment.game.status == GameStatus::Connecting {
            return Ok(None);
        }
        if environment.game.status == GameStatus::Disconnected {
            return Ok(Some("Disconnected from server".to_string()));
        }
        if let GameStatus::Won(team) = environment.game.status {
            return Ok(Some(format!("{:?} win!", team)));
        }
        Ok(environment.game.simulate(elapsed))
    }

    pub fn draw(&mut self) -> Result<Option<String>, JsValue> {
        let environment = self
            .environment
            .lock()
            .expect("Internal Error: could not get a lock on the game");
        if environment.game.status == GameStatus::Connecting {
            return Ok(None);
        }
        if environment.game.status == GameStatus::Disconnected {
            return Ok(Some("Disconnected from server".to_string()));
        }
        Ok(environment.canvas.draw(&environment.game))
    }
}

#[wasm_bindgen]
pub fn make_game() -> Result<GameWrapper, JsValue> {
    let CanvasInfo {
        context,
        width,
        height,
    } = get_canvas_info()
        .map_err(|e| JsValue::from(format!("Error initializing canvas: {}", e)))?;
    let ws = WebSocket::new("ws://localhost:3012")?;

    let my_uuid: UUID = random_uuid();
    let starting_position_seed = randf64();
    let local_player = Player {
        uuid: my_uuid,
        color: Color::random(),
        dead: false,
        position: Position {
            x: 275.0 + (100.0 * (starting_position_seed * 2.0 * f64::consts::PI).sin()),
            y: 275.0 + (100.0 * (starting_position_seed * 2.0 * f64::consts::PI).cos()),
        },
        impostor: false,
        // 6 random tasks
        tasks: (0..6)
            .map(|_| Task {
                position: Position {
                    x: random_up_to(width).max(30.0).min(width - 30.0),
                    y: random_up_to(height).max(30.0).min(height - 30.0),
                },
                finished: false,
            })
            .collect(),
        speed: Speed { dx: 0.0, dy: 0.0 },
    };

    let wrapper_wrapper: Arc<Mutex<Option<Arc<Mutex<GameEnvironment>>>>> =
        Arc::new(Mutex::new(None));

    {
        let wrapper_wrapper_clone = wrapper_wrapper.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            // Starting with assuming text messages. Can make efficient later (bson?).
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let strng: String = txt.into();
                let message: Message = match serde_json::from_str(&strng) {
                    Ok(m) => m,
                    Err(e) => {
                        console_log!("Unable to deserialize {:?} – {:?}", strng, e);
                        return;
                    }
                };
                console_log!("message from network: {:?}", message);
                let option_wrapped = &wrapper_wrapper_clone.lock().unwrap();
                let wrapper = option_wrapped.as_ref().unwrap();
                let mut environment = wrapper.lock().unwrap();
                match environment.game.handle_msg(message) {
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
            let mut environment = wrapper.lock().unwrap();
            environment
                .game
                .disconnected()
                .expect("Could not handle disconnection in game");
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        // TODO: wait on socket to connect before returning.
        let wrapper_wrapper_clone = wrapper_wrapper.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            console_log!("socket opened");
            let option_wrapped = &wrapper_wrapper_clone.lock().unwrap();
            let wrapper = &option_wrapped.as_ref().unwrap();
            let mut environment = wrapper.lock().unwrap();
            environment
                .game
                .connected()
                .expect("Could not handle connected in game");
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();
    }

    let wrapper = GameWrapper {
        environment: Arc::new(Mutex::new(GameEnvironment {
            game: Game {
                status: GameStatus::Connecting,
                speed: 2.0,
                task_distance: 32.0,
                kill_distance: 64.0,
                inputs: InputState {
                    up: false,
                    down: false,
                    left: false,
                    right: false,
                    kill: false,
                    activate: false,
                    report: false,
                    play: false,
                },
                local_player_uuid: Some(my_uuid),
                players: vec![local_player],
                bodies: Vec::new(),
                socket: Box::new(WebSocketTx { socket: ws }),
            },
            canvas: Canvas {
                context,
                width,
                height,
            },
        })),
    };
    {
        let mut wrapped = wrapper_wrapper.lock().unwrap();
        *wrapped = Some(wrapper.environment.clone());
    }
    Ok(wrapper)
}
