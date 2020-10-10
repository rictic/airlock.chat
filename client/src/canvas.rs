use crate::*;
use rust_us_core::Task;
use rust_us_core::HEIGHT;
use rust_us_core::WIDTH;
use rust_us_core::{DeadBody, GameAsPlayer, GameStatus, Player};
use std::error::Error;
use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

fn find_canvas_in_document() -> Result<
  (
    web_sys::HtmlCanvasElement,
    web_sys::CanvasRenderingContext2d,
  ),
  Box<dyn Error>,
> {
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

  Ok((canvas, context))
}

pub struct Canvas {
  width: f64,
  height: f64,
  camera: Camera,
  context: web_sys::CanvasRenderingContext2d,
  canvas_element: web_sys::HtmlCanvasElement,
}

#[derive(Clone, Copy, Debug)]
struct Camera {
  zoom: f64,
  left: f64,
  right: f64,
  top: f64,
  bottom: f64,
}
impl Camera {
  fn offset(self, x: f64, y: f64) -> (f64, f64) {
    let x = (x - self.left) * self.zoom;
    let y = (y - self.top) * self.zoom;
    (x, y)
  }
}
impl Canvas {
  pub fn new(
    context: web_sys::CanvasRenderingContext2d,
    canvas_element: web_sys::HtmlCanvasElement,
  ) -> Canvas {
    Canvas {
      context,
      canvas_element,
      camera: Camera {
        top: 0.0,
        left: 0.0,
        bottom: HEIGHT,
        right: WIDTH,
        zoom: 1.0,
      },
      width: WIDTH,
      height: HEIGHT,
    }
  }

  pub fn find_in_document() -> Result<Canvas, JsValue> {
    let (canvas_element, context) =
      find_canvas_in_document().map_err(|e| JsValue::from(format!("{}", e)))?;
    Ok(Canvas::new(context, canvas_element))
  }

  fn set_dimensions(&mut self) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("Could not get window")?;
    let ratio = window.device_pixel_ratio();
    let width: f64 = window.inner_width()?.as_f64().unwrap();
    let height = window.inner_height()?.as_f64().unwrap();
    self
      .canvas_element
      .set_width((width * ratio).floor() as u32);
    self
      .canvas_element
      .set_height((height * ratio).floor() as u32);
    self.context.scale(ratio, ratio)?;
    self.width = width;
    self.height = height;
    Ok(())
  }

  // Draws the current game state.
  pub fn draw(&mut self, game: Arc<Mutex<GameAsPlayer>>) -> Result<(), Box<dyn Error>> {
    self.set_dimensions().map_err(|e| format!("{:?}", e))?;
    let game = game.lock().unwrap();
    let context = &self.context;
    // Frame the canvas.
    context.clear_rect(0.0, 0.0, self.width, self.height);
    context.begin_path();
    context.rect(0.0, 0.0, self.width, self.height);
    context.set_fill_style(&JsValue::from_str("#f3f3f3"));
    context.fill();
    if game.game.status == GameStatus::Connecting {
      return Ok(());
    }

    // Move the
    self.camera = match game.local_player() {
      None => {
        // the spectator sees all
        Camera {
          zoom: 1.0,
          left: 0.0,
          top: 0.0,
          right: self.width,
          bottom: self.height,
        }
      }
      Some(p) => {
        let zoom = 2.0;
        let map_width = self.width / zoom;
        let map_height = self.height / zoom;
        // Players see the area around them
        Camera {
          zoom,
          left: p.position.x - (map_width / 2.0),
          right: p.position.x + (map_width / 2.0),
          top: p.position.y - (map_height / 2.0),
          bottom: p.position.y + (map_height / 2.0),
        }
      }
    };

    self.context.set_line_width(self.camera.zoom);

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
      if game.game.status == GameStatus::Playing {
        for task in local_player.tasks.iter() {
          if task.finished {
            continue;
          }
          self.draw_task(*task, local_player.impostor)?;
        }
      }
    }
    for body in game.game.bodies.iter() {
      self.draw_body(*body)?;
    }
    for (_, player) in game.game.players.iter() {
      if show_dead_people || !player.dead {
        self.draw_player(player)?
      }
    }

    Ok(())
  }

  fn draw_player(&self, player: &Player) -> Result<(), &'static str> {
    // draw circle
    self.context.begin_path();
    let radius = 10.0;
    self.move_to(player.position.x + radius, player.position.y);
    self
      .arc(
        player.position.x,
        player.position.y,
        radius,
        0.0,
        std::f64::consts::PI * 2.0,
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

    // draw name
    if !player.dead {
      self.context.set_text_align("center");
      self.context.set_font(&format!(
        "{}px Arial Black",
        (12.0 * self.camera.zoom).floor()
      ));
      self.context.set_fill_style(&JsValue::from("#000"));
      self.context.set_stroke_style(&JsValue::from("#fff"));
      self.context.set_line_width(self.camera.zoom);
      self.stroke_text(&player.name, player.position.x, player.position.y - 14.0)?;
      self.fill_text(&player.name, player.position.x, player.position.y - 14.0)?;
    }

    Ok(())
  }

  fn draw_body(&self, body: DeadBody) -> Result<(), &'static str> {
    self.context.begin_path();
    let radius = 10.0;
    self.move_to(body.position.x + radius, body.position.y);
    self
      .arc(body.position.x, body.position.y, radius, 0.0, PI * 1.0)
      .map_err(|_| "Failed to draw a circle.")?;
    self
      .context
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
    self.move_to(pos.x + (len / 2.0), pos.y);
    self.line_to(pos.x, pos.y + height);
    self.line_to(pos.x + len, pos.y + height);
    self.line_to(pos.x + (len / 2.0), pos.y);
    if fake {
      self.context.set_fill_style(&JsValue::from("#ffa50244"));
      self.context.set_stroke_style(&JsValue::from("#00000044"));
    } else {
      self.context.set_fill_style(&JsValue::from("#ffa502"));
      self.context.set_stroke_style(&JsValue::from("#000000"));
    }
    self.context.fill();
    self.context.stroke();
    self.move_to(pos.x + (len / 2.0), pos.y + 3.0);
    self.line_to(pos.x + (len / 2.0), pos.y + 9.0);
    self.move_to(pos.x + (len / 2.0), pos.y + 10.0);
    self.line_to(pos.x + (len / 2.0), pos.y + 12.0);
    self.context.stroke();
    Ok(())
  }

  fn circle(&self, x: f64, y: f64, radius: f64) -> Result<(), &'static str> {
    self.context.begin_path();
    self.move_to(x + radius, y);
    self
      .arc(x, y, radius, 0.0, PI * 2.0)
      .map_err(|_| "Failed to draw a circle.")?;
    self.context.stroke();
    self.context.fill();
    Ok(())
  }

  // Like context.move_to but corrects for the window
  fn move_to(&self, x: f64, y: f64) {
    let (x, y) = self.camera.offset(x, y);
    self.context.move_to(x, y);
  }

  fn line_to(&self, x: f64, y: f64) {
    let (x, y) = self.camera.offset(x, y);
    self.context.line_to(x, y);
  }

  fn arc(
    &self,
    x: f64,
    y: f64,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
  ) -> Result<(), &'static str> {
    let (x, y) = self.camera.offset(x, y);
    let radius = radius * self.camera.zoom;
    self
      .context
      .arc(x, y, radius, start_angle, end_angle)
      .map_err(|_| "Failed to draw a circle.")
  }

  fn fill_text(&self, text: &str, x: f64, y: f64) -> Result<(), &'static str> {
    let (x, y) = self.camera.offset(x, y);
    self
      .context
      .fill_text(&text, x, y)
      .map_err(|_| "Failed to fill in text.")
  }

  fn stroke_text(&self, text: &str, x: f64, y: f64) -> Result<(), &'static str> {
    let (x, y) = self.camera.offset(x, y);
    self
      .context
      .stroke_text(&text, x, y)
      .map_err(|_| "Failed to draw text outline.")
  }
}
