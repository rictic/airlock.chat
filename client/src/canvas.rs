use crate::*;
use core::time::Duration;
use rust_us_core::*;
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

  fn get_global_camera(dimensions: (f64, f64)) -> Self {
    let (width, height) = dimensions;
    Self {
      // this isn't the right zoom, it should be relative to
      // canvas width and height
      zoom: 1.0,
      left: 0.0,
      top: 0.0,
      right: width,
      bottom: height,
    }
  }

  fn centered_on_point(dimensions: (f64, f64), center: Position) -> Self {
    // Likewise, this zoom shouldn't be constant
    let zoom = 2.0;
    let (width, height) = dimensions;
    let map_width = width / zoom;
    let map_height = height / zoom;
    // Players see the area around them
    Camera {
      zoom,
      left: center.x - (map_width / 2.0),
      right: center.x + (map_width / 2.0),
      top: center.y - (map_height / 2.0),
      bottom: center.y + (map_height / 2.0),
    }
  }

  fn roughly_track_object(
    self,
    (width, height): (f64, f64),
    map: &Map,
    tracked: Position,
  ) -> Camera {
    // This is what this article calls the 'camera-window' system
    // https://www.gamasutra.com/blogs/ItayKeren/20150511/243083/Scroll_Back_The_Theory_and_Practice_of_Cameras_in_SideScrollers.php

    let mut result = Camera {
      // This zoom shouldn't be constant
      zoom: 2.0,
      left: self.left,
      right: self.right,
      top: self.top,
      bottom: self.bottom,
    };

    // Imagine a smallish rectangle in the center of the screen.
    // If the tracked object stays within that rectangle, the camera stays
    // fixed. When it leaves the rectangle, the camera moves the minimal amount
    // to keep it in there.
    let (dx, dy) = {
      let x_center = width / 2.0;
      let y_center = height / 2.0;
      let bounding_left = x_center - (width * 0.075);
      let bounding_right = x_center + (width * 0.075);
      let bounding_top = y_center - (height * 0.075);
      let bounding_bottom = y_center + (height * 0.075);

      let (x, y) = result.offset(tracked.x, tracked.y);
      let mut dx = 0.0;

      if x < bounding_left {
        dx = x - (bounding_left);
      } else if x > bounding_right {
        dx = x - (bounding_right - 1.0);
      }
      let mut dy = 0.0;
      if y < bounding_top {
        dy = y - bounding_top;
      } else if y > bounding_bottom {
        dy = y - bounding_bottom;
      }
      (dx / self.zoom, dy / self.zoom)
    };

    if dx != 0.0 {
      result.left += dx;
      result.right += dx;
    }
    if dy != 0.0 {
      result.top += dy;
      result.bottom += dy;
    }

    let oob_limit = 20.0;
    result.snap_to_edge(map, oob_limit);

    // The camera jerked abruptly? Maybe the player teleported. To help anchor them, try to center
    // the player in the camera.
    if (result.left - self.left).abs() > 30.0 || (result.top - self.top).abs() > 30.0 {
      let mut centered = Camera::centered_on_point((width, height), tracked);
      centered.snap_to_edge(map, oob_limit);
      return centered;
    }
    result
  }

  fn snap_to_edge(&mut self, map: &Map, oob_limit: f64) {
    // See edge-snapping from
    // https://www.gamasutra.com/blogs/ItayKeren/20150511/243083/Scroll_Back_The_Theory_and_Practice_of_Cameras_in_SideScrollers.php
    //
    // Still need to resolve what happens with viewports that are almost the size
    // of the map, or even larger.
    // as the player moves around within them, the camera could mostly stay still,
    // but this causes them to snap to the edge sometimes. Hm.
    if self.left <= -oob_limit {
      console_log!("Snapped to left edge");
      let correction = -self.left - oob_limit;
      self.left += correction;
      self.right += correction;
    } else if self.right >= (map.width() + oob_limit) {
      console_log!("Snapped to right edge");
      let correction = self.right - (map.width() + oob_limit);
      self.left -= correction;
      self.right -= correction;
    }
    if self.top <= -oob_limit {
      console_log!("Snapped to top edge");
      let correction = -self.top - oob_limit;
      self.top += correction;
      self.bottom += correction;
    } else if self.bottom >= (map.height() + oob_limit) {
      console_log!("Snapped to bottom edge");
      let correction = self.bottom - (map.height() + oob_limit);
      self.top -= correction;
      self.bottom -= correction;
    }
  }
}

struct WindowDimensions {
  width: f64,
  height: f64,
  device_pixel_ratio: f64,
}
fn get_window_dimensions() -> Result<WindowDimensions, JsValue> {
  let window = web_sys::window().ok_or("Could not get window")?;
  let ratio = window.device_pixel_ratio();
  let width: f64 = window
    .inner_width()?
    .as_f64()
    .ok_or("Could not inner_width as number?")?;
  let height = window
    .inner_height()?
    .as_f64()
    .ok_or("Could not inner_height as number?")?;
  Ok(WindowDimensions {
    width,
    height,
    device_pixel_ratio: ratio,
  })
}

impl Canvas {
  pub fn new(
    context: web_sys::CanvasRenderingContext2d,
    canvas_element: web_sys::HtmlCanvasElement,
  ) -> Result<Canvas, JsValue> {
    let WindowDimensions { width, height, .. } = get_window_dimensions()?;
    Ok(Canvas {
      context,
      canvas_element,
      camera: Camera::get_global_camera((width, height)),
      width,
      height,
    })
  }

  pub fn find_in_document() -> Result<Canvas, JsValue> {
    let (canvas_element, context) =
      find_canvas_in_document().map_err(|e| JsValue::from(format!("{}", e)))?;
    Canvas::new(context, canvas_element)
  }

  fn set_dimensions(&mut self) -> Result<(), JsValue> {
    let WindowDimensions {
      width,
      height,
      device_pixel_ratio,
    } = get_window_dimensions()?;
    self
      .canvas_element
      .set_width((width * device_pixel_ratio).floor() as u32);
    self
      .canvas_element
      .set_height((height * device_pixel_ratio).floor() as u32);
    self.context.scale(device_pixel_ratio, device_pixel_ratio)?;
    self.width = width;
    self.height = height;
    Ok(())
  }

  // Draws the current game state.
  pub fn draw(&mut self, game: Arc<Mutex<Option<GameAsPlayer>>>) -> Result<(), JsValue> {
    self.set_dimensions().map_err(|e| format!("{:?}", e))?;
    let game = game.lock().unwrap();
    // Frame the canvas.
    self.context.clear_rect(0.0, 0.0, self.width, self.height);
    if game.is_none() {
      return Ok(());
    }
    let game = game.as_ref().unwrap();
    match &game.state.status {
      GameStatus::Connecting | GameStatus::Disconnected => (),
      GameStatus::Won(team) => {
        self.context.begin_path();
        self.context.set_text_align("center");
        self.context.set_text_baseline("middle");
        self.context.set_font(&format!(
          "{}px Arial Black",
          (48.0 * self.camera.zoom).floor()
        ));
        self.context.set_fill_style(&JsValue::from("#000"));
        self.context.set_stroke_style(&JsValue::from("#fff"));
        self.context.set_line_width(self.camera.zoom * 4.0);
        let middle = (self.width / 2.0, self.height / 2.0);

        let message = match game.has_won(team) {
          Some(true) => "You win!".to_string(),
          Some(false) => "You lose!".to_string(),
          None => format!("{:?} win!", team),
        };
        self.context.stroke_text(&message, middle.0, middle.1)?;
        self.context.fill_text(&message, middle.0, middle.1)?;
      }
      GameStatus::Lobby | GameStatus::Playing(PlayState::Night) => {
        self.draw_night(&game)?;
      }
      GameStatus::Playing(PlayState::Day(n)) => {
        self.camera = Camera::get_global_camera((self.width, self.height));
        let voting_ui_state = match &game.contextual_state {
          ContextualState::Voting(v) => Some(v),
          _ => None,
        };
        self.draw_day(&game, n, voting_ui_state)?
      }
    };

    let font_height = 24.0;
    self
      .context
      .set_font(&format!("{}px Arial Black", font_height));
    self.context.set_line_width(4.0);
    self.context.set_text_align("left");
    self.context.set_text_baseline("middle");
    let mut messages: Vec<Message> = game
      .displayed_messages
      .iter()
      .rev()
      .filter(|m| m.ready_to_display())
      .map(|m| m.message.clone())
      .collect();
    if game.state.status == GameStatus::Lobby {
      messages.push(Message::PlainString(format!(
        "In the lobby. {}/10 players",
        game.state.players.len()
      )));
      messages.push(Message::PlainString(format!("Press P to start")));
    }
    for (i, message) in messages.into_iter().enumerate() {
      self.context.begin_path();
      let text_pos = (
        30.0,
        self.height - (30.0 + (font_height + 5.0) * (i as f64)),
      );
      match &message {
        Message::PlainString(s) => {
          self.context.set_stroke_style(&JsValue::from("#fff"));
          self.context.set_fill_style(&JsValue::from("#000"));
          self.context.stroke_text(s, text_pos.0, text_pos.1)?;
          self.context.fill_text(s, text_pos.0, text_pos.1)?;
        }
        Message::FormattingString(parts) => {
          let mut x = 0.0;
          for part in parts {
            self.context.set_fill_style(
              &part
                .color
                .map(|c| c.to_str().into())
                .unwrap_or_else(|| JsValue::from("#000")),
            );
            self.context.set_stroke_style(
              &part
                .color
                .map(|c| c.text_outline_color().into())
                .unwrap_or_else(|| JsValue::from("#fff")),
            );
            let metrics = self.context.measure_text(&part.text)?;
            self
              .context
              .stroke_text(&part.text, text_pos.0 + x, text_pos.1)?;
            self
              .context
              .fill_text(&part.text, text_pos.0 + x, text_pos.1)?;
            x += metrics.width();
          }
        }
      }
    }
    Ok(())
  }

  fn draw_night(&mut self, game: &GameAsPlayer) -> Result<(), JsValue> {
    self.context.begin_path();
    self.context.rect(0.0, 0.0, self.width, self.height);
    self.context.set_fill_style(&JsValue::from_str("#f3f3f3"));
    self.context.fill();
    let local_player = game.local_player();
    self.camera = match local_player {
      None => {
        // the spectator sees all
        Camera::get_global_camera((self.width, self.height))
      }
      Some(p) => {
        // Center the camera on the player
        self
          .camera
          .roughly_track_object((self.width, self.height), &game.state.map, p.position)
      }
    };

    {
      let zero = self.camera.offset(0.0, 0.0);
      if zero.0 > 0.0 {
        self.context.begin_path();
        self.context.rect(0.0, 0.0, zero.0, self.height);
        self.context.set_fill_style(&"#000".into());
        self.context.fill();
      }
      if zero.1 > 0.0 {
        self.context.begin_path();
        self.context.rect(0.0, 0.0, self.width, zero.1);
        self.context.set_fill_style(&"#000".into());
        self.context.fill();
      }
      let bot_right = self
        .camera
        .offset(game.state.map.width(), game.state.map.height());
      if bot_right.0 < self.width {
        self.context.begin_path();
        self
          .context
          .rect(bot_right.0, 0.0, self.width - bot_right.0, self.height);
        self.context.set_fill_style(&"#000".into());
        self.context.fill();
      }
      if bot_right.1 < self.height {
        self.context.begin_path();
        self
          .context
          .rect(0.0, bot_right.1, self.width, self.height - bot_right.1);
        self.context.set_fill_style(&"#000".into());
        self.context.fill();
      }
    }

    self.context.set_line_width(self.camera.zoom);

    for shape in game.state.map.static_geometry.iter() {
      self.draw_shape(shape)?;
    }

    let show_dead_people = match game.local_player() {
      None => true,
      Some(p) => p.dead || p.impostor,
    };

    let can_see = |other: &Position| match local_player {
      Some(p) => p.can_see(&game.state.settings, other),
      None => {
        return true;
      }
    };

    // Draw tasks, then bodies, then players on top, so tasks are behind everything, then
    // bodies, then imps. That way imps can stand on top of bodies.
    // However maybe we should instead draw items from highest to lowest, vertically?
    if let Some(local_player) = game.local_player() {
      for task in local_player.tasks.iter() {
        if task.finished {
          continue;
        }
        self.draw_task(*task, local_player.impostor)?;
      }
    }
    for body in game.state.bodies.iter() {
      if can_see(&body.position) {
        self.draw_body(*body)?;
      }
    }
    for (_, player) in game.state.players.iter() {
      if (show_dead_people || !player.dead) && can_see(&player.position) {
        self.draw_player(player)?
      }
    }

    // Draw a semitransparant overlay for fog of war.
    let vision = game.vision();
    if let Some(vision) = vision {
      if let Some(player) = local_player {
        let vision = vision * self.camera.zoom;
        let (x, y) = self.camera.offset(player.position.x, player.position.y);
        let fringe = Player::radius() * 2.5 * self.camera.zoom;
        let gradient = self
          .context
          .create_radial_gradient(x, y, vision, x, y, vision - fringe)?;
        gradient.add_color_stop(0.0, "#000c")?;
        gradient.add_color_stop(0.4, "#0009")?;
        gradient.add_color_stop(1.0, "#0000")?;

        self.context.begin_path();
        self.context.set_fill_style(&gradient);
        self.context.rect(0.0, 0.0, self.width, self.height);
        self.context.fill();
      }
    }

    Ok(())
  }

  fn draw_shape(&self, shape: &Shape) -> Result<(), JsValue> {
    match shape {
      Shape::Circle {
        radius,
        center,
        fill_color,
        outline_width,
        outline_color,
      } => {
        self.context.begin_path();
        self.context.set_fill_style(&JsValue::from(*fill_color));
        self
          .context
          .set_stroke_style(&JsValue::from(*outline_color));
        self
          .context
          .set_line_width(outline_width * self.camera.zoom);
        let (x, y) = self.camera.offset(center.x, center.y);
        self
          .context
          .arc(x, y, radius * self.camera.zoom, 0.0, 2.0 * PI)?;
        self.context.stroke();
        self.context.fill();
      }
    }
    Ok(())
  }

  fn draw_player(&self, player: &Player) -> Result<(), &'static str> {
    // draw circle
    self.context.begin_path();
    let radius = Player::radius();
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

  fn draw_day(
    &mut self,
    game: &GameAsPlayer,
    day_state: &DayState,
    voting_state: Option<&VotingUiState>,
  ) -> Result<(), JsValue> {
    let line_width = 25.0 * self.camera.zoom;
    let half_line_width = line_width / 2.0;
    self.context.begin_path();
    self.context.rect(
      half_line_width,
      half_line_width,
      self.width - line_width,
      self.height - line_width,
    );
    self.context.set_stroke_style(&JsValue::from_str("#333"));
    self.context.set_line_width(line_width);
    self.context.stroke();

    // draw boxes
    let num_rows = 1 + (((Color::all().len() as f64) / 2.0).ceil() as u32);
    self.context.begin_path();
    self.context.move_to(self.width / 2.0, 0.0);
    self.context.line_to(self.width / 2.0, self.height);
    self.context.stroke();

    let row_height = (self.height - line_width) / (num_rows as f64);
    let row_width = self.width / 2.0;
    for i in 1..num_rows {
      self.context.begin_path();
      let line_height = ((i as f64) * row_height) + half_line_width;
      self.context.move_to(0.0, line_height);
      self.context.line_to(self.width, line_height);
      self.context.stroke();
    }

    let row_inner_height = row_height - line_width;
    let row_inner_width = (self.width - (line_width * 3.0)) / 2.0;
    for (i, (uuid, player)) in game.state.players.iter().enumerate() {
      let row = i / 2;
      let column = i % 2;
      let top_left = (
        ((row_width - half_line_width) * column as f64) + line_width,
        (row_height * row as f64) + line_width,
      );

      let mut is_selected = false;
      if let Some(voting_state) = voting_state {
        if voting_state.highlighted_player == Some(VoteTarget::Player { uuid: *uuid }) {
          is_selected = true;
        }
      }

      if player.dead || is_selected {
        // Draw backing color
        self.context.begin_path();
        self
          .context
          .rect(top_left.0, top_left.1, row_inner_width, row_inner_height);
        self.context.set_fill_style(
          &(if is_selected {
            JsValue::from("#33d")
          } else {
            JsValue::from("#666")
          }),
        );
        self.context.fill();
      }

      // Draw avatars
      self.context.begin_path();
      let avatar_radius = ((row_inner_height / 2.0) - (line_width)).max(0.0);
      self.context.arc(
        top_left.0 + (row_inner_height / 2.0),
        top_left.1 + (row_inner_height / 2.0),
        avatar_radius,
        0.0,
        if player.dead { PI } else { PI * 2.0 },
      )?;
      self
        .context
        .set_fill_style(&JsValue::from(player.color.to_str()));
      self.context.set_line_width(8.0 * self.camera.zoom);
      self.context.set_stroke_style(&JsValue::from("#000"));
      self.context.stroke();
      self.context.fill();

      // Draw an "I voted" sticker once they've voted
      if !player.dead && day_state.votes.contains_key(uuid) {
        self.context.begin_path();
        let sticker_pos = (
          top_left.0 + (row_inner_height / 2.0) + (0.37 * avatar_radius),
          top_left.1 + (row_inner_height / 2.0) + (0.14 * avatar_radius),
        );
        self.context.ellipse(
          sticker_pos.0,
          sticker_pos.1,
          0.30 * avatar_radius,
          0.20 * avatar_radius,
          0.0,
          0.0,
          PI * 2.0,
        )?;
        self.context.set_fill_style(&JsValue::from("#fff"));
        self.context.set_line_width(4.0 * self.camera.zoom);
        self.context.set_stroke_style(&JsValue::from("#000"));
        self.context.stroke();
        self.context.fill();

        self
          .context
          .set_font(&format!("{}px Arial Black", (0.12 * avatar_radius)));
        self.context.set_fill_style(&JsValue::from("#e11"));
        self.context.set_text_align("center");
        self.context.set_text_baseline("middle");
        self
          .context
          .fill_text("I voted!", sticker_pos.0, sticker_pos.1)?;
      }

      // Draw names
      self.context.set_font(&format!(
        "{}px Arial Black",
        (24.0 * self.camera.zoom).floor()
      ));
      self.context.begin_path();
      self.context.set_line_width(5.0 * self.camera.zoom);
      self.context.set_stroke_style(&JsValue::from("#000"));
      self.context.set_text_align("left");
      self.context.set_text_baseline("bottom");
      self.context.stroke_text(
        &player.name,
        top_left.0 + avatar_radius + (3.5 * line_width),
        top_left.1 + (1.5 * line_width),
      )?;
      self.context.set_fill_style(&JsValue::from("#fff"));
      self.context.fill_text(
        &player.name,
        top_left.0 + avatar_radius + (3.5 * line_width),
        top_left.1 + (1.5 * line_width),
      )?;
    }

    {
      // Draw the 'skip' option
      let top_left = (line_width, (row_height * 5.0) + line_width);

      let mut is_selected = false;
      if let Some(voting_state) = voting_state {
        if voting_state.highlighted_player == Some(VoteTarget::Skip) {
          is_selected = true;
        }
      }

      if is_selected {
        // Draw backing color
        self.context.begin_path();
        self
          .context
          .rect(top_left.0, top_left.1, row_inner_width, row_inner_height);
        self.context.set_fill_style(&JsValue::from("#33d"));
        self.context.fill();
      }

      self.context.set_font(&format!(
        "{}px Arial Black",
        (24.0 * self.camera.zoom).floor()
      ));
      self.context.begin_path();
      self.context.set_line_width(5.0 * self.camera.zoom);
      self.context.set_stroke_style(&JsValue::from("#000"));
      self.context.set_text_align("left");
      self.context.set_text_baseline("middle");
      self.context.set_fill_style(&JsValue::from("#fff"));
      let text_pos = (
        top_left.0 + (1.0 * line_width),
        top_left.1 + (row_inner_height / 2.0),
      );
      self.context.stroke_text("Skip", text_pos.0, text_pos.1)?;
      self.context.fill_text("Skip", text_pos.0, text_pos.1)?;
    }

    {
      // Draw the time remaining
      let top_right = (self.width - line_width, (row_height * 5.0) + line_width);

      self.context.set_font(&format!(
        "{}px Arial Black",
        (22.0 * self.camera.zoom).floor()
      ));
      self.context.begin_path();
      self.context.set_line_width(5.0 * self.camera.zoom);
      self.context.set_stroke_style(&JsValue::from("#000"));
      self.context.set_text_align("right");
      self.context.set_text_baseline("middle");
      self
        .context
        .set_fill_style(&&if day_state.time_remaining < Duration::from_secs(20) {
          JsValue::from("#d22")
        } else {
          JsValue::from("#fff")
        });
      let text_pos = (
        top_right.0 - (1.5 * line_width),
        top_right.1 + (row_inner_height / 2.0),
      );
      let text = format!("{}s remaining to vote", day_state.time_remaining.as_secs());
      self.context.stroke_text(&text, text_pos.0, text_pos.1)?;
      self.context.fill_text(&text, text_pos.0, text_pos.1)?;
    }

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
  ) -> Result<(), JsValue> {
    let (x, y) = self.camera.offset(x, y);
    let radius = radius * self.camera.zoom;
    self.context.arc(x, y, radius, start_angle, end_angle)
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
