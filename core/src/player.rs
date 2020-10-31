use crate::game_state::Vector2d;
use crate::*;
use core::time::Duration;
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

impl InputState {
  // Returns an InputState with buttons set to true if they
  // aren't pressed on self, but are set on newer_input.
  fn get_new_presses(&self, newer_input: InputState) -> InputState {
    InputState {
      up: !self.up && newer_input.up,
      down: !self.down && newer_input.down,
      left: !self.left && newer_input.left,
      right: !self.right && newer_input.right,
      kill: !self.kill && newer_input.kill,
      activate: !self.activate && newer_input.activate,
      report: !self.report && newer_input.report,
      play: !self.play && newer_input.play,
      skip_back: !self.skip_back && newer_input.skip_back,
      skip_forward: !self.skip_forward && newer_input.skip_forward,
      pause_playback: !self.pause_playback && newer_input.pause_playback,
    }
  }
}

// A game from the perspective of a specific player
pub struct GameAsPlayer {
  pub my_uuid: UUID,
  inputs: InputState,
  pub state: GameState,
  pub socket: Box<dyn GameTx>,
  pub contextual_state: ContextualState,
  pub displayed_messages: Vec<DisplayMessage>,
}

// A game from the perspective of a particular player.
impl GameAsPlayer {
  pub fn new(uuid: UUID, socket: Box<dyn GameTx>) -> GameAsPlayer {
    GameAsPlayer {
      state: GameState::new(),
      inputs: InputState::default(),
      contextual_state: ContextualState::Blank,
      my_uuid: uuid,
      socket,
      displayed_messages: Vec::new(),
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

  pub fn simulate(&mut self, elapsed: Duration) -> bool {
    // Tick down time for our displayed messages, and drop the ones
    // whose durations have expired.
    for message in self.displayed_messages.iter_mut() {
      message.pass_time(elapsed);
    }
    self.displayed_messages.retain(|m| !m.is_expired());
    self.state.simulate(elapsed)
  }

  // Take the given inputs from the local player
  pub fn take_input(&mut self, new_input: InputState) -> Result<(), String> {
    match &self.state.status {
      GameStatus::Lobby | GameStatus::Playing(PlayState::Night) => self.take_night_input(new_input),
      GameStatus::Playing(PlayState::Voting(day_state)) => {
        let updated_voting_state = self.take_day_input(day_state, new_input)?;
        if let Some(updated_voting_state) = updated_voting_state {
          match &mut self.contextual_state {
            ContextualState::Blank => return Err("Internal error, bad contextual state".into()),
            ContextualState::Voting(voting) => {
              *voting = updated_voting_state;
            }
          }
        }
        self.inputs = new_input;
        Ok(())
      }
      GameStatus::Playing(PlayState::TallyingVotes(_))
      | GameStatus::Playing(PlayState::ViewingOutcome(_))
      | GameStatus::Connecting
      | GameStatus::Won(_)
      | GameStatus::Disconnected => {
        // Nothing to do
        Ok(())
      }
    }
  }

  fn take_night_input(&mut self, new_input: InputState) -> Result<(), String> {
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
      speed_changed = new_speed != player.velocity;
      player.velocity = new_speed;
    }

    // This way we don't send a MoveMessage unless movement keys actually changed,
    // reducing data leakage to HAXXORZ.
    if speed_changed {
      let player = self.local_player().unwrap();
      self.socket.send(&ClientToServerMessage::Move(MoveMessage {
        speed: player.velocity,
        position: player.position,
      }))?;
    }
    Ok(())
  }

  fn take_day_input(
    &self,
    day_state: &VotingState,
    new_input: InputState,
  ) -> Result<Option<VotingUiState>, String> {
    let pressed = self.inputs.get_new_presses(new_input);
    let player = match self.local_player() {
      None => {
        // Spectators don't get a vote.
        return Ok(None);
      }
      Some(p) => p,
    };
    if player.dead {
      // The dead don't get a vote.
      return Ok(None);
    }
    let has_voted = day_state.votes.contains_key(&player.uuid);
    if has_voted {
      // Nothing to do but wait if you've already voted.
      return Ok(None);
    }
    let mut voting_state = match self.contextual_state {
      ContextualState::Voting(voting) => voting,
      ContextualState::Blank => {
        return Err(
          "Internal Error: expected to be in Voting contextual state during the day.".to_string(),
        )
      }
    };

    let mut vote_targets: Vec<TargetInVotingTable> = self
      .state
      .players
      .iter()
      .enumerate()
      .filter(|(_, (_, p))| !p.dead)
      .map(|(idx, (uuid, _p))| TargetInVotingTable::new(idx, VoteTarget::Player { uuid: *uuid }))
      .collect();
    vote_targets.push(TargetInVotingTable::new(10, VoteTarget::Skip));
    match voting_state.highlighted_player {
      None => {
        if pressed.up || pressed.down || pressed.left || pressed.right {
          // Nothing was highlighted, so highlight the first target player.
          voting_state.highlighted_player = vote_targets.first().map(|vt| vt.target);
        }
      }
      Some(highlighted) => {
        let mut highlighted: TargetInVotingTable = *vote_targets
          .iter()
          .find(|vt| vt.target == highlighted)
          .ok_or_else(|| "Internal Error: Highlighting a nonexistant player?".to_string())?;
        if pressed.up {
          let mut closest_same_column_above: Option<TargetInVotingTable> = None;
          let mut closest_above: Option<TargetInVotingTable> = None;
          for p in vote_targets.iter() {
            if p.y >= highlighted.y {
              break; // no longer above
            }
            if p.x == highlighted.x {
              closest_same_column_above = Some(*p);
            } else {
              closest_above = Some(*p);
            }
          }
          highlighted =
            closest_same_column_above.unwrap_or_else(|| closest_above.unwrap_or(highlighted));
        }
        if pressed.down {
          let mut closest_same_column_below: Option<TargetInVotingTable> = None;
          let mut closest_below: Option<TargetInVotingTable> = None;
          for p in vote_targets.iter() {
            if p.y <= highlighted.y {
              continue; // not below
            }
            if p.x == highlighted.x && closest_same_column_below.is_none() {
              closest_same_column_below = Some(*p);
            } else if closest_below.is_none() {
              closest_below = Some(*p);
            }
          }
          highlighted =
            closest_same_column_below.unwrap_or_else(|| closest_below.unwrap_or(highlighted));
        }
        if pressed.left && highlighted.x == 1 {
          let mut closest_left_column_above: Option<TargetInVotingTable> = None;
          let mut first_in_left_column: Option<TargetInVotingTable> = None;
          for p in vote_targets.iter() {
            if p.x != 0 {
              continue; // not in left column
            }
            if p.y <= highlighted.y {
              closest_left_column_above = Some(*p);
            } else if first_in_left_column.is_none() {
              first_in_left_column = Some(*p);
            }
          }
          highlighted = closest_left_column_above
            .unwrap_or_else(|| first_in_left_column.unwrap_or(highlighted));
        }
        if pressed.right && highlighted.x == 0 {
          let mut closest_right_column_above: Option<TargetInVotingTable> = None;
          let mut first_in_right_column: Option<TargetInVotingTable> = None;
          for p in vote_targets.iter() {
            if p.x != 1 {
              continue; // not in right column
            }
            if p.y <= highlighted.y {
              closest_right_column_above = Some(*p);
            } else if first_in_right_column.is_none() {
              first_in_right_column = Some(*p);
            }
          }
          highlighted = closest_right_column_above
            .unwrap_or_else(|| first_in_right_column.unwrap_or(highlighted));
        }
        voting_state.highlighted_player = Some(highlighted.target);
      }
    }
    if pressed.activate {
      if let Some(target) = voting_state.highlighted_player {
        self.socket.send(&ClientToServerMessage::Vote { target })?;
        voting_state.highlighted_player = None;
      }
    }
    Ok(Some(voting_state))
  }

  fn get_speed(&self) -> Velocity {
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
    Velocity { dx, dy }
  }

  fn kill_player_near(&mut self, position: Position) -> Result<(), String> {
    let mut killed_player: Option<DeadBody> = None;
    let mut closest_distance = self.state.settings.kill_distance;

    for (_, player) in self.state.players.iter_mut() {
      if player.impostor || player.uuid == self.my_uuid || player.dead {
        continue;
      }

      let distance = position.distance(&player.position);
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
      let distance = position.distance(&task.position);
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
      let distance = position.distance(&body.position);
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
      _ => self.update_status(GameStatus::Disconnected),
    };
    Ok(())
  }

  // Returns the distance that this client should be able to see.
  // Returns None is they should see everything.
  pub fn vision(&self) -> Option<f64> {
    self
      .local_player()
      .map(|p| p.vision(&self.state.settings, &self.state.status))
      .flatten()
  }

  // Returns whether this client is for a player that won.
  // Returns None if the client is a spectator.
  pub fn has_won(&self, winning_team: &Team) -> Option<bool> {
    match self.local_player() {
      None => None,
      Some(p) => {
        let impostor_won = match winning_team {
          Team::Crew => false,
          Team::Impostors => true,
        };
        Some(p.impostor == impostor_won)
      }
    }
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
        self.update_status(status);
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
                velocity: speed,
              } = player;
              local_player.name = name;
              local_player.color = color;
              local_player.dead = dead;
              local_player.impostor = impostor;
              local_player.tasks = tasks;
              // Always trust our local speed over the server
              if player.uuid != self.my_uuid {
                local_player.velocity = speed;
              }
              // Avoid jitter by ignoring position updates (and instead use local reconning
              // based on speeds) unless the distance is greater than some small amount.
              if position.distance(&local_player.position) > 30.0 {
                local_player.position = position;
              }
            }
          }
        }
      }
      ServerToClientMessage::Replay(_recorded_game) => {
        // Nothing to handle here. The JS client handles this itself.
      }
      ServerToClientMessage::DisplayMessage(display_message) => {
        self.displayed_messages.push(display_message);
      }
    }
    Ok(())
  }

  fn start(&mut self) -> Result<(), String> {
    self.socket.send(&ClientToServerMessage::StartGame())?;
    Ok(())
  }

  fn update_status(&mut self, new_status: GameStatus) {
    if !self.state.status.is_same_kind(&new_status) {
      self.inputs = InputState::default();
    }
    if let GameStatus::Playing(PlayState::Voting(_)) = new_status {
      match self.contextual_state {
        ContextualState::Voting(_) => (),
        _ => self.contextual_state = ContextualState::Voting(VotingUiState::default()),
      }
    } else {
      match self.contextual_state {
        ContextualState::Blank => (),
        _ => self.contextual_state = ContextualState::Blank,
      }
    }
    self.state.status = new_status;
  }
}

// This is terrible design lol. Integrate with game.status maybe?
pub enum ContextualState {
  Blank,
  Voting(VotingUiState),
}

#[derive(Default, Debug, Copy, Clone)]
pub struct VotingUiState {
  pub highlighted_player: Option<VoteTarget>,
}

pub trait GameTx {
  fn send(&self, message: &ClientToServerMessage) -> Result<(), String>;
}

#[derive(Clone, Copy)]
struct TargetInVotingTable {
  x: usize,
  y: usize,
  target: VoteTarget,
}
impl TargetInVotingTable {
  fn new(index: usize, target: VoteTarget) -> Self {
    TargetInVotingTable {
      x: index % 2,
      y: index / 2,
      target,
    }
  }
}
