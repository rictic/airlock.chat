use crate::ServerToClientMessage;
use crate::*;
use core::time::Duration;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::BufRead;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecordedGame {
  // The version of the software this was recorded with.
  pub version: String,
  pub game_id: UUID,
  pub entries: Vec<RecordingEntry>,
}
impl RecordedGame {
  pub fn new(game_id: UUID, entries: Vec<RecordingEntry>) -> Self {
    Self {
      version: get_version_sha().to_string(),
      entries,
      game_id,
    }
  }

  pub fn read(mut reader: impl BufRead) -> Result<RecordedGame, RecordingReadError> {
    let header = RecordedGame::read_header(&mut reader)?;
    // We promise the header line will be backwards compatible, but the rest
    // of the file won't be. Can't go on unless the versions match.
    if header.version != get_version_sha() {
      return Err(RecordingReadError::VersionMismatch {
        found: header.version,
      });
    }
    let lines = reader.lines();
    let mut entries = Vec::new();
    for line in lines {
      entries.push(serde_json::from_str(&line?)?);
    }
    Ok(RecordedGame {
      version: header.version,
      game_id: header.game_id,
      entries,
    })
  }

  pub fn read_header(reader: &mut impl BufRead) -> Result<ReplayFileHeader, RecordingReadError> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // We expect the first line of a replay to be a json object with a field 'version' that's a string.
    let header: ReplayFileHeader = serde_json::from_str(&line)?;
    if header.file_type != "airlock replay" {
      return Err(RecordingReadError::Err(
        "Replay file did not have a file_type key with value \"airlock replay\"".into(),
      ));
    }
    Ok(header)
  }
}
pub enum RecordingReadError {
  VersionMismatch { found: String },
  Err(Box<dyn Error>),
}
impl std::convert::From<std::io::Error> for RecordingReadError {
  fn from(err: std::io::Error) -> Self {
    RecordingReadError::Err(err.into())
  }
}
impl std::convert::From<serde_json::Error> for RecordingReadError {
  fn from(err: serde_json::Error) -> Self {
    RecordingReadError::Err(err.into())
  }
}
pub struct GameRecordingWriter<W>
where
  W: Write,
{
  inner: W,
}
impl<W> GameRecordingWriter<W>
where
  W: Write,
{
  pub fn new(mut inner: W, version: &str, game_id: UUID) -> Result<Self, std::io::Error> {
    let header = ReplayFileHeader::new(version.to_string(), game_id);
    inner.write_all(serde_json::to_string(&header).unwrap().as_bytes())?;
    inner.write_all(b"\n")?;
    Ok(Self { inner })
  }

  pub fn write(&mut self, event: &RecordingEvent) -> Result<(), std::io::Error> {
    self
      .inner
      .write_all(serde_json::to_string(&event).unwrap().as_bytes())?;
    self.inner.write_all(b"\n")?;
    Ok(())
  }
}

// Unlike other parts of the protocol, we want this to be backwards and forwards
// compatible.
#[derive(Serialize, Deserialize)]
pub struct ReplayFileHeader {
  pub version: String,
  pub game_id: UUID,
  file_type: String,
}
impl ReplayFileHeader {
  fn new(version: String, game_id: UUID) -> Self {
    Self {
      file_type: "airlock replay".to_string(),
      version,
      game_id,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecordingEntry {
  pub since_start: Duration,
  pub event: RecordingEvent,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RecordingEvent {
  Message(PlaybackMessage),
  Disconnect(UUID),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlaybackMessage {
  pub sender: UUID,
  pub message: ClientToServerMessage,
  pub decision: Option<ServerDecision>,
}
#[derive(Debug, Clone)]
pub enum MaybeDecisionIfPlayingBackRecording {
  LiveGame,
  Playback(Option<ServerDecision>),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerDecision {
  StartInfo(StartInfo),
  NewPlayerPosition(Position),
}

struct PlaybackBroadcaster {
  pending_messages: Arc<Mutex<Vec<ServerToClientMessage>>>,
}
impl Broadcaster for PlaybackBroadcaster {
  fn broadcast(&self, message: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
    let mut messages = self.pending_messages.lock().unwrap();
    messages.push(message.clone());
    Ok(())
  }
  fn send_to_player(&self, _: &UUID, _: &ServerToClientMessage) -> Result<(), Box<dyn Error>> {
    // ??? what to do here
    Ok(())
  }
}
pub struct PlaybackTx {}
impl GameTx for PlaybackTx {
  fn send(&self, _: &ClientToServerMessage) -> Result<(), String> {
    Ok(()) // do nothing
  }
}

pub struct PlaybackServer {
  current_time: Duration,
  current_index: usize,
  paused: bool,
  recording: RecordedGame,
  game_server: GameServer,
  pending_messages: Arc<Mutex<Vec<ServerToClientMessage>>>,
}

impl PlaybackServer {
  pub fn new(recording: RecordedGame) -> Self {
    let pending_messages = Arc::new(Mutex::new(Vec::new()));
    let mut game_server = GameServer::new(
      UUID::random(),
      Box::new(PlaybackBroadcaster {
        pending_messages: pending_messages.clone(),
      }),
      None,
    );
    game_server.version = recording.version.clone();
    game_server.state.status = GameStatus::Lobby;
    Self {
      current_time: Duration::from_secs(0),
      current_index: 0,
      paused: false,
      game_server,
      recording,
      pending_messages,
    }
  }

  pub fn restart(&mut self) {
    let mut game_server = GameServer::new(
      self.recording.game_id,
      Box::new(PlaybackBroadcaster {
        pending_messages: self.pending_messages.clone(),
      }),
      None,
    );
    game_server.version = self.recording.version.clone();
    game_server.state.status = GameStatus::Lobby;
    self.game_server = game_server;
    self.current_time = Duration::from_millis(0);
    self.current_index = 0;
  }

  pub fn duration(&self) -> Duration {
    // Assume that the final message marks the end of the recording.
    self
      .recording
      .entries
      .last()
      .map(|e| e.since_start)
      .unwrap_or_else(|| Duration::from_secs(0))
  }

  pub fn current_time(&self) -> Duration {
    self.current_time
  }

  pub fn skip_to(
    &mut self,
    from_start: Duration,
    player: &mut GameAsPlayer,
  ) -> Result<(), Box<dyn Error>> {
    if from_start < self.current_time {
      self.restart();
      player.displayed_messages.clear();
    }
    while self.current_time < from_start {
      let elapsed = Duration::from_millis(16);
      let finished = self.simulate(elapsed, player, true)?;
      player.simulate(elapsed);
      if finished {
        // the simulation is done, can't skip past this point
        break;
      }
    }
    self.game_server.broadcast_snapshot()?;
    self.deliver_messages(player)?;
    Ok(())
  }

  pub fn toggle_pause(&mut self) {
    self.paused = !self.paused;
  }

  pub fn paused(&self) -> bool {
    self.paused
  }

  pub fn simulate(
    &mut self,
    elapsed: Duration,
    player: &mut GameAsPlayer,
    force: bool,
  ) -> Result<bool, Box<dyn Error>> {
    if self.paused && !force {
      return Ok(true);
    }
    let new_time = self.current_time + elapsed;
    let mut server_messages = 0;
    loop {
      let entry = match self.recording.entries.get(self.current_index) {
        // Done with entries, just uh... continue simulating!
        None => break,
        Some(entry) => entry,
      };
      if entry.since_start > new_time {
        break;
      }
      self.current_index += 1;
      // Ensure that we handle other kinds of events later.
      #[allow(clippy::infallible_destructuring_match)]
      match &entry.event {
        RecordingEvent::Message(message) => {
          self.game_server.handle_message_playback(message)?;
        }
        RecordingEvent::Disconnect(uuid) => {
          self.game_server.disconnected(*uuid)?;
        }
      };
      server_messages += 1;
    }
    if self.game_server.state.status.finished() && server_messages == 0 {
      return Ok(true);
    }
    self.game_server.simulate(elapsed)?;
    self.current_time = new_time;
    self.deliver_messages(player)?;
    Ok(false)
  }

  fn deliver_messages(&mut self, player: &mut GameAsPlayer) -> Result<(), Box<dyn Error>> {
    let mut pending_messages = self.pending_messages.lock().unwrap();
    for message in pending_messages.iter() {
      player.handle_msg(message.clone())?;
    }
    pending_messages.clear();
    Ok(())
  }
}
