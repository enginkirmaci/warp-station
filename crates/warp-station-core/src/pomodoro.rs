//! The warp-pomodoro client: the live snapshot (door first, `state.json`
//! after), the token-authenticated loopback door, and the detached launcher.
//!
//! Warp Station owns none of the timer's state. The app at
//! `~/.warp/pomodoro` publishes `state.json` and, while it runs, the family's
//! door (`ipc.port` + `ipc.token`) with the commands `status`, `start`,
//! `pause`, `reset`, `skip` and `focus`; the contract is
//! `plans/warp-pomodoro.md` §1. A refused or absent endpoint means the app is
//! not running, not that anything failed: callers fall back to the state file
//! and never raise a toast over a closed app.

use crate::exe;
use crate::process::DetachedSpawn;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use warpui_kit_platform::door::{self, MAX_LINE};

/// The app's own folder under the suite root, `~/.warp/pomodoro`.
pub fn pomodoro_dir() -> PathBuf {
    crate::paths::AppPaths::pomodoro_dir()
}

/// The live snapshot's file inside `dir`.
pub fn state_file_in(dir: &Path) -> PathBuf {
    dir.join("state.json")
}

/// The live snapshot's file inside [`pomodoro_dir`].
pub fn state_file() -> PathBuf {
    state_file_in(&pomodoro_dir())
}

/// The timer phase the snapshot names. The contract has three; a value a
/// newer app adds reads as [`PomodoroPhase::Unknown`] instead of failing the
/// whole snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroPhase {
    #[default]
    Work,
    ShortBreak,
    LongBreak,
    Unknown,
}

impl PomodoroPhase {
    /// The page's chip label: the contract's phase in the family's words.
    pub fn label(self) -> &'static str {
        match self {
            Self::Work => "Work",
            Self::ShortBreak => "Short break",
            Self::LongBreak => "Long break",
            Self::Unknown => "Unknown",
        }
    }
}

// a hand-edited or newer phase must not invalidate the snapshot
impl<'de> Deserialize<'de> for PomodoroPhase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "work" => Self::Work,
            "short_break" => Self::ShortBreak,
            "long_break" => Self::LongBreak,
            _ => Self::Unknown,
        })
    }
}

/// `state.json` exactly as warp-pomodoro writes it (§1.1). Station only
/// reads it, so unknown fields stay tolerated: a newer app may add one
/// without this reader failing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PomodoroSnapshot {
    pub schema_version: u32,
    pub phase: PomodoroPhase,
    pub running: bool,
    pub remaining_seconds: u32,
    pub phase_seconds: u32,
    pub long_break_every: u32,
    pub completed_in_cycle: u32,
    pub today_work: u32,
    pub week_work: u32,
    /// RFC 3339 local time of the write; the page shows it as "last seen"
    /// when the app is not running.
    pub updated_at: String,
    pub version: String,
}

/// One snapshot and where it came from: a live door answer, or the file a
/// closed app left behind. Only the live form counts as "running".
#[derive(Clone, Debug, PartialEq)]
pub struct PomodoroStatus {
    pub snapshot: PomodoroSnapshot,
    pub live: bool,
}

/// The door's command vocabulary (§1.2). Station is the client; the app owns
/// the meaning of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PomodoroCommand {
    Status,
    Start,
    Pause,
    Reset,
    Skip,
    Focus,
}

impl PomodoroCommand {
    /// The exact wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Start => "start",
            Self::Pause => "pause",
            Self::Reset => "reset",
            Self::Skip => "skip",
            Self::Focus => "focus",
        }
    }
}

/// One command's answer: the state commands answer the new snapshot, `focus`
/// answers an empty frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PomodoroAnswer {
    Snapshot(PomodoroSnapshot),
    Focused,
}

/// How long connect/read/write waits on the door. A live app on loopback
/// answers in about a millisecond; anything past this reads as "not running"
/// so a dropped socket can never hold the page.
const TIMEOUT: Duration = Duration::from_millis(400);

/// The `state.json` reader: None while the app has not written one yet (or
/// the file is mid-replace and not parseable).
pub fn read_state_file_in(dir: &Path) -> Option<PomodoroSnapshot> {
    let text = std::fs::read_to_string(state_file_in(dir)).ok()?;
    serde_json::from_str(text.trim()).ok()
}

/// [`read_state_file_in`] over the default folder.
pub fn read_state_file() -> Option<PomodoroSnapshot> {
    read_state_file_in(&pomodoro_dir())
}

/// The door's `status`, or `Err("not running")` when no endpoint answers.
pub fn status_in(dir: &Path) -> Result<PomodoroSnapshot, String> {
    match send_in(dir, PomodoroCommand::Status)? {
        PomodoroAnswer::Snapshot(snapshot) => Ok(snapshot),
        PomodoroAnswer::Focused => Err("not running".to_string()),
    }
}

/// [`status_in`] over the default folder.
pub fn status() -> Result<PomodoroSnapshot, String> {
    status_in(&pomodoro_dir())
}

/// Sends one command and returns its answer. A missing, refused or
/// stale endpoint is `Err("not running")` — the page's normal closed state.
pub fn send(command: PomodoroCommand) -> Result<PomodoroAnswer, String> {
    send_in(&pomodoro_dir(), command)
}

/// [`send`] with the endpoint folder exposed for tests and probes.
pub fn send_in(dir: &Path, command: PomodoroCommand) -> Result<PomodoroAnswer, String> {
    let answer = call(dir, command.as_str())?;
    if command == PomodoroCommand::Focus {
        return Ok(PomodoroAnswer::Focused);
    }
    let snapshot =
        serde_json::from_value(answer).map_err(|err| format!("bad pomodoro snapshot: {err}"))?;
    Ok(PomodoroAnswer::Snapshot(snapshot))
}

/// The live status: the door when it answers, otherwise the `state.json` a
/// closed app left (`live: false`). None when neither exists.
pub fn read_status_in(dir: &Path) -> Option<PomodoroStatus> {
    if let Ok(snapshot) = status_in(dir) {
        return Some(PomodoroStatus {
            snapshot,
            live: true,
        });
    }
    read_state_file_in(dir).map(|snapshot| PomodoroStatus {
        snapshot,
        live: false,
    })
}

/// [`read_status_in`] over the default folder.
pub fn read_status() -> Option<PomodoroStatus> {
    read_status_in(&pomodoro_dir())
}

/// One authenticated door call: the auth frame, the request frame, one answer
/// frame. Blocking `std::net`, bounded by [`TIMEOUT`].
fn call(dir: &Path, command: &str) -> Result<Value, String> {
    let (port, token) = door::read_endpoint_in(dir).ok_or_else(|| "not running".to_string())?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let stream =
        TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|_| "not running".to_string())?;
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|_| "not running".to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|_| "not running".to_string())?;
    let mut reader = BufReader::new(stream);

    write_frame(reader.get_mut(), &json!({"auth": token}))?;
    let welcome = read_frame(&mut reader)?;
    if !welcome.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        // a rejected token means the app this endpoint belongs to restarted
        // and published a new one: from here it is simply not reachable
        return Err("not running".to_string());
    }

    let frame = door::request_frame(1, command, json!({}));
    write_frame(reader.get_mut(), &frame)?;
    let answer = read_frame(&mut reader)?;
    if answer.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(answer);
    }
    let error = answer
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown door error");
    Err(error.to_string())
}

fn write_frame(stream: &mut TcpStream, frame: &Value) -> Result<(), String> {
    stream
        .write_all(format!("{frame}\n").as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|_| "not running".to_string())
}

/// One NDJSON frame, capped at the door's [`MAX_LINE`]; a closed or oversized
/// line reads as "not running" for a read error and as a bad answer for a
/// live but misbehaving door.
fn read_frame(reader: &mut BufReader<TcpStream>) -> Result<Value, String> {
    let mut limited = reader.take(MAX_LINE as u64 + 1);
    let mut line = String::new();
    let read = limited
        .read_line(&mut line)
        .map_err(|_| "not running".to_string())?;
    if read == 0 {
        return Err("not running".to_string());
    }
    if line.len() > MAX_LINE {
        return Err("oversized pomodoro frame".to_string());
    }
    serde_json::from_str(line.trim()).map_err(|err| format!("bad pomodoro answer: {err}"))
}

/// Launches warp-pomodoro detached from Warp Station's lifetime: the window
/// outlives this app. `start` asks for `--start` (§1.3), the Start verb when
/// the door is down; a plain launch opens the app paused.
pub fn launch(executable: &str, start: bool) -> Result<(), String> {
    let Some(path) = exe::locate(executable.trim()) else {
        return Err(format!(
            "warp-pomodoro executable not found: {}",
            executable.trim()
        ));
    };
    let mut command = std::process::Command::new(&path);
    command.args(launch_args(start));
    // the window outlives Warp Station: fully detached, no watcher
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn_detached()
        .map_err(|err| format!("could not start warp-pomodoro: {err}"))
}

/// The arguments after the executable: `--start` only when the page asked to
/// start a session.
pub fn launch_args(start: bool) -> Vec<OsString> {
    if start {
        vec![OsString::from("--start")]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Mutex;

    /// A fake warp-pomodoro door over a temp endpoint dir: it validates the
    /// token exactly as the app does, logs every command it receives, and
    /// answers the state commands with one canned snapshot.
    struct FakeDoor {
        dir: tempfile::TempDir,
        received: std::sync::Arc<Mutex<Vec<String>>>,
        server: Option<std::thread::JoinHandle<()>>,
    }

    impl FakeDoor {
        fn start(connections: usize) -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let token = door::random_token();
            door::write_private(&door::token_file_in(dir.path()), &token).expect("token");
            door::write_private(&door::port_file_in(dir.path()), &format!("{port}\n"))
                .expect("port");
            let received = std::sync::Arc::new(Mutex::new(Vec::new()));
            let log = received.clone();
            let expected = token.clone();
            let server = std::thread::spawn(move || {
                for _ in 0..connections {
                    let Ok((mut stream, _)) = listener.accept() else {
                        return;
                    };
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut auth = String::new();
                    if reader.read_line(&mut auth).is_err() || auth.trim().is_empty() {
                        continue;
                    }
                    let provided = serde_json::from_str::<Value>(auth.trim())
                        .ok()
                        .and_then(|v| v.get("auth").and_then(Value::as_str).map(str::to_string))
                        .unwrap_or_default();
                    if !door::token_matches(&provided, &expected) {
                        let _ = stream.write_all(b"{\"ok\":false,\"error\":\"auth\"}\n");
                        continue;
                    }
                    if stream.write_all(b"{\"ok\":true}\n").is_err() {
                        continue;
                    }
                    let mut request = String::new();
                    if reader.read_line(&mut request).is_err() || request.trim().is_empty() {
                        continue;
                    }
                    let Ok(request) = serde_json::from_str::<Value>(request.trim()) else {
                        continue;
                    };
                    let cmd = request
                        .get("cmd")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let id = request.get("id").and_then(Value::as_u64).unwrap_or(1);
                    log.lock().expect("log").push(cmd.clone());
                    let answer = if cmd == "focus" {
                        json!({})
                    } else {
                        snapshot_json(true)
                    };
                    let frame = door::answer_frame(id, Ok(answer));
                    let _ = stream.write_all(format!("{frame}\n").as_bytes());
                }
            });
            Self {
                dir,
                received,
                server: Some(server),
            }
        }

        fn wait(mut self) -> Vec<String> {
            if let Some(server) = self.server.take() {
                server.join().expect("fake door thread");
            }
            self.received.lock().expect("log").clone()
        }
    }

    fn snapshot_json(running: bool) -> Value {
        json!({
            "schema_version": 1,
            "phase": "work",
            "running": running,
            "remaining_seconds": 1443,
            "phase_seconds": 1500,
            "long_break_every": 4,
            "completed_in_cycle": 2,
            "today_work": 5,
            "week_work": 18,
            "updated_at": "2026-11-15T20:45:14+03:00",
            "version": "0.1.2"
        })
    }

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn status_reads_the_snapshot_over_an_authenticated_door() {
        let door = FakeDoor::start(1);
        let snapshot = status_in(door.dir.path()).expect("a live door answers");
        assert_eq!(snapshot.phase, PomodoroPhase::Work);
        assert!(snapshot.running);
        assert_eq!(snapshot.remaining_seconds, 1443);
        assert_eq!(snapshot.completed_in_cycle, 2);
        assert_eq!(snapshot.today_work, 5);
        assert_eq!(snapshot.week_work, 18);
        assert_eq!(snapshot.version, "0.1.2");
        assert_eq!(door.wait(), ["status"]);
    }

    #[test]
    fn start_and_pause_are_one_command_each() {
        let door = FakeDoor::start(2);
        assert!(matches!(
            send_in(door.dir.path(), PomodoroCommand::Start),
            Ok(PomodoroAnswer::Snapshot(_))
        ));
        assert!(matches!(
            send_in(door.dir.path(), PomodoroCommand::Pause),
            Ok(PomodoroAnswer::Snapshot(_))
        ));
        assert_eq!(door.wait(), ["start", "pause"]);
    }

    #[test]
    fn a_wrong_token_reads_as_not_running() {
        let door = FakeDoor::start(1);
        std::fs::write(door::token_file_in(door.dir.path()), "not-the-token\n").expect("token");
        assert_eq!(status_in(door.dir.path()), Err("not running".to_string()));
        let received = door.wait();
        assert!(
            received.is_empty(),
            "an unauthenticated request reaches no command: {received:?}"
        );
    }

    #[test]
    fn an_absent_endpoint_reads_as_not_running() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(status_in(dir.path()), Err("not running".to_string()));
        assert!(send_in(dir.path(), PomodoroCommand::Pause).is_err());
    }

    #[test]
    fn a_stale_endpoint_falls_back_to_the_state_file() {
        let dir = scratch();
        // the published port has no listener: the app died without clearing it
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        door::write_private(&door::token_file_in(dir.path()), "token").expect("token");
        door::write_private(&door::port_file_in(dir.path()), &format!("{port}\n")).expect("port");
        std::fs::write(
            state_file_in(dir.path()),
            serde_json::to_string(&snapshot_json(false)).expect("json"),
        )
        .expect("state");

        let status = read_status_in(dir.path()).expect("the file answers");
        assert!(!status.live, "a dead endpoint is never live");
        assert!(!status.snapshot.running);
        assert_eq!(status.snapshot.remaining_seconds, 1443);
    }

    #[test]
    fn the_state_file_reader_parses_the_contract_and_tolerates_new_fields() {
        let dir = scratch();
        std::fs::write(
            state_file_in(dir.path()),
            r#"{
                "schema_version": 1,
                "phase": "short_break",
                "running": false,
                "remaining_seconds": 301,
                "phase_seconds": 300,
                "long_break_every": 4,
                "completed_in_cycle": 3,
                "today_work": 5,
                "week_work": 18,
                "updated_at": "2026-11-15T20:45:14+03:00",
                "version": "0.1.2",
                "a_later_field": true
            }"#,
        )
        .expect("state");
        let snapshot = read_state_file_in(dir.path()).expect("parse");
        assert_eq!(snapshot.phase, PomodoroPhase::ShortBreak);
        assert_eq!(snapshot.phase_seconds, 300);
        assert_eq!(snapshot.remaining_seconds, 301);
        assert_eq!(snapshot.phase.label(), "Short break");
    }

    #[test]
    fn an_unreadable_phase_keeps_the_snapshot() {
        let phase: PomodoroPhase = serde_json::from_str("\"deep_work\"").expect("tolerated");
        assert_eq!(phase, PomodoroPhase::Unknown);
    }

    #[test]
    fn launch_asks_for_start_only_when_asked() {
        assert!(launch_args(false).is_empty());
        assert_eq!(
            launch_args(true)
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["--start"]
        );
    }

    #[test]
    fn a_missing_executable_names_the_setting() {
        let err = launch("warp-station-definitely-missing-warp-pomodoro", true).unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn the_app_folder_sits_beside_the_other_apps() {
        let _guard = crate::test_support::HOME_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let home = tempfile::tempdir().expect("scratch home");
        let original = std::env::var("HOME").ok();
        // SAFETY: process-global env, held only for this test's body; the
        // lock keeps a sibling HOME reader from seeing this scratch root
        unsafe { std::env::set_var("HOME", home.path()) };

        assert_eq!(pomodoro_dir(), home.path().join(".warp").join("pomodoro"));

        match original {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}
