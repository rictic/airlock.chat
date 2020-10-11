use crate::*;
use std::collections::BTreeSet;

// The state of user input at some point in time. i.e. what buttons is
// the user holding down?
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct InputState {
  pub up: bool,
  pub down: bool,
  pub left: bool,
  pub right: bool,
  pub kill: bool,
  pub activate: bool,
  pub report: bool,
  pub play: bool,
  pub skip_back: bool,
  pub skip_forward: bool,
  pub pause_playback: bool,
}

// A game from the perspective of a specific player
pub struct GameAsPlayer {
  pub my_uuid: UUID,
  inputs: InputState,
  pub state: GameState,
  pub socket: Box<dyn GameTx>,
}

pub trait GameTx {
  fn send(&self, message: &ClientToServerMessage) -> Result<(), String>;
}

// A game from the perspective of a particular player.
impl GameAsPlayer {
  pub fn new(uuid: UUID, socket: Box<dyn GameTx>) -> GameAsPlayer {
    GameAsPlayer {
      state: GameState::new(),
      inputs: InputState::default(),
      my_uuid: uuid,
      socket,
    }
  }

  // Is there a way to avoid duplicating the logic between local_player and local_player_mut?
  pub fn local_player(&self) -> Option<&Player> {
    self.state.players.get(&self.my_uuid)
  }

  fn local_player_mut(&mut self) -> Option<&mut Player> {
    self.state.players.get_mut(&self.my_uuid)
  }

  pub fn inputs(&self) -> InputState {
    self.inputs
  }

  // Take the given inputs from the local player
  pub fn take_input(&mut self, new_input: InputState) -> Result<(), String> {
    let current_input = self.inputs;
    let player = match self.local_player_mut() {
      None => return Ok(()),
      Some(p) => p,
    };
    if new_input == current_input {
      return Ok(()); // quick exit for the boring case
    }
    // Read the parts of the local player that we care about.
    let is_killing = player.impostor && !current_input.kill && new_input.kill;
    let position = player.position;
    let activating = !current_input.activate && new_input.activate;
    let reporting = !current_input.report && new_input.report;
    let starting_play =
      self.state.status == GameStatus::Lobby && !current_input.play && new_input.play;
    self.inputs = new_input;
    // ok, we're done touching player at this point. we redeclare it
    // below so we can use it again, next time mutably.

    if is_killing {
      self.kill_player_near(position)?;
    }
    if activating {
      self.activate_near(position)?;
    }
    if starting_play {
      self.start()?;
    }
    if reporting {
      self.report_body_near(position)?;
    }

    let speed_changed: bool;
    {
      let new_speed = self.get_speed();
      let player = self.local_player_mut().unwrap();
      speed_changed = new_speed != player.speed;
      player.speed = new_speed;
    }

    // This way we don't send a MoveMessage unless movement keys actually changed,
    // reducing data leakage to HAXXORZ.
    if speed_changed {
      let player = self.local_player().unwrap();
      self.socket.send(&ClientToServerMessage::Move(MoveMessage {
        speed: player.speed,
        position: player.position,
      }))?;
    }
    Ok(())
  }

  fn get_speed(&self) -> Speed {
    let mut dx = 0.0;
    let mut dy = 0.0;
    if self.inputs.up && !self.inputs.down {
      dy = -self.state.settings.speed
    } else if self.inputs.down {
      dy = self.state.settings.speed
    }
    if self.inputs.left && !self.inputs.right {
      dx = -self.state.settings.speed
    } else if self.inputs.right {
      dx = self.state.settings.speed
    }
    Speed { dx, dy }
  }

  fn kill_player_near(&mut self, position: Position) -> Result<(), String> {
    let mut killed_player: Option<DeadBody> = None;
    let mut closest_distance = self.state.settings.kill_distance;

    for (_, player) in self.state.players.iter_mut() {
      if player.impostor || player.uuid == self.my_uuid || player.dead {
        continue;
      }

      let distance = position.distance(player.position);
      if distance < closest_distance {
        killed_player = Some(DeadBody {
          position: player.position,
          color: player.color,
        });
        closest_distance = distance;
      }
    }

    if let Some(body) = killed_player {
      self.state.note_death(body)?;
      self.socket.send(&ClientToServerMessage::Killed(body))?;
      // Move the killer on top of the new body.
      if let Some(player) = self.local_player_mut() {
        player.position = body.position;
      }
    }

    Ok(())
  }

  fn activate_near(&mut self, position: Position) -> Result<(), String> {
    let mut closest_distance = self.state.settings.task_distance;
    let local_player = match self.local_player_mut() {
      Some(player) => player,
      None => return Ok(()),
    };
    let is_imp = local_player.impostor;

    let mut finished_task: Option<FinishedTask> = None;
    for (index, task) in local_player.tasks.iter().enumerate() {
      let distance = position.distance(task.position);
      if distance < closest_distance {
        finished_task = Some(FinishedTask { index });
        closest_distance = distance;
      }
    }
    if let Some(finished_task) = finished_task {
      if !is_imp {
        self.state.note_finished_task(self.my_uuid, finished_task)?;
        self
          .socket
          .send(&ClientToServerMessage::FinishedTask(finished_task))?;
      }
    }
    Ok(())
  }

  fn report_body_near(&mut self, position: Position) -> Result<(), String> {
    let mut closest_distance = self.state.settings.report_distance;
    let mut nearest_body_color: Option<Color> = None;
    for body in self.state.bodies.iter() {
      let distance = position.distance(body.position);
      if distance < closest_distance {
        nearest_body_color = Some(body.color);
        closest_distance = distance;
      }
    }
    if let Some(color) = nearest_body_color {
      self.socket.send(&ClientToServerMessage::ReportBody {
        dead_body_color: color,
      })?;
    }
    Ok(())
  }

  pub fn disconnected(&mut self) -> Result<(), String> {
    match self.state.status {
      GameStatus::Won(_) => (), // do nothing, this is expected
      _ => self.state.status = GameStatus::Disconnected,
    };
    Ok(())
  }

  pub fn handle_msg(&mut self, message: ServerToClientMessage) -> Result<(), String> {
    console_log!("Player handling message: {}", message.kind());
    match message {
      ServerToClientMessage::Welcome {
        connection_id: uuid,
      } => {
        self.my_uuid = uuid;
      }
      ServerToClientMessage::Snapshot(Snapshot {
        status,
        bodies,
        players,
      }) => {
        self.state.status = status;
        self.state.bodies = bodies;
        // handle disconnections
        let server_uuids: BTreeSet<_> = players.iter().map(|p| p.uuid).collect();
        let local_uuids: BTreeSet<_> = self.state.players.iter().map(|(u, _)| *u).collect();
        for uuid in local_uuids.difference(&server_uuids) {
          self.state.players.remove(uuid);
        }

        for player in players {
          match self.state.players.get_mut(&player.uuid) {
            None => {
              self.state.players.insert(player.uuid, player);
            }
            Some(local_player) => {
              let Player {
                name,
                uuid: _uuid,
                color,
                dead,
                impostor,
                tasks,
                position,
                speed,
              } = player;
              local_player.name = name;
              local_player.color = color;
              local_player.dead = dead;
              local_player.impostor = impostor;
              local_player.tasks = tasks;
              // Always trust our local speed over the server
              if player.uuid != self.my_uuid {
                local_player.speed = speed;
              }
              // Avoid jitter by ignoring position updates (and instead use local reconning
              // based on speeds) unless the distance is greater than some small amount.
              if position.distance(local_player.position) > 30.0 {
                local_player.position = position;
              }
            }
          }
        }
      }
      ServerToClientMessage::Replay(_recorded_game) => {
        // Nothing to handle here. The JS client handles this itself.
      }
    }
    Ok(())
  }

  fn start(&mut self) -> Result<(), String> {
    self.socket.send(&ClientToServerMessage::StartGame())?;
    Ok(())
  }
}
