//! The Pomodoro section: its rail item sits between Summary and Tools, the
//! page renders the snapshot its door answers, Pause writes the door command,
//! and a closed app with no state file reads as "Not running" with no toast.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::io::{BufRead, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;
use warpui_kit_platform::door;

/// The env hooks this probe drives (`WARP_STATION_POMODORO_DIR`,
/// `WARP_STATION_PAGE`) are process-wide, so the tests take turns.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    draw(cx);
}

/// Scratch settings for one probe: a stable per-test file under the temp dir,
/// never the developer's live `~/.warp/station/settings.json`.
fn scratch_service(
    name: &str,
    json: &str,
) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::AtomicU64;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-pomodoro-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, json).expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

/// A fake warp-pomodoro door: the family's auth handshake, a JSON snapshot as
/// every state answer, and a log of the commands it received. The listener
/// stays up until the probe drops it, so the page's once-a-second poll can
/// make as many calls as it likes.
struct FakeDoor {
    dir: tempfile::TempDir,
    log: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl FakeDoor {
    fn start() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let token = door::random_token();
        door::write_private(&door::token_file_in(dir.path()), &token).expect("token");
        door::write_private(&door::port_file_in(dir.path()), &format!("{port}\n")).expect("port");

        let snapshot = Arc::new(Mutex::new(snapshot_json(true, 1443)));
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let snapshot = snapshot.clone();
            let log = log.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(stream, &token, &snapshot, &log),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            dir,
            log,
            stop,
            server: Some(server),
        }
    }

    fn commands(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }
}

impl Drop for FakeDoor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn snapshot_json(running: bool, remaining: u32) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "phase": "work",
        "running": running,
        "remaining_seconds": remaining,
        "phase_seconds": 1500,
        "long_break_every": 4,
        "completed_in_cycle": 2,
        "today_work": 5,
        "week_work": 18,
        "updated_at": "2026-11-15T20:45:14+03:00",
        "version": "0.1.2"
    })
}

fn serve(
    mut stream: TcpStream,
    token: &str,
    snapshot: &Mutex<serde_json::Value>,
    log: &Mutex<Vec<String>>,
) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_stream);

    let mut auth = String::new();
    if reader.read_line(&mut auth).is_err() {
        return;
    }
    let provided = serde_json::from_str::<serde_json::Value>(auth.trim())
        .ok()
        .and_then(|value| {
            value
                .get("auth")
                .and_then(|auth| auth.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    if !door::token_matches(&provided, token) {
        let _ = stream.write_all(b"{\"ok\":false,\"error\":\"auth\"}\n");
        return;
    }
    if stream.write_all(b"{\"ok\":true}\n").is_err() {
        return;
    }

    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_str::<serde_json::Value>(request.trim()) else {
        return;
    };
    let cmd = request
        .get("cmd")
        .and_then(|cmd| cmd.as_str())
        .unwrap_or_default()
        .to_string();
    let id = request.get("id").and_then(|id| id.as_u64()).unwrap_or(1);
    log.lock().expect("log").push(cmd.clone());

    let answer = {
        let mut snapshot = snapshot.lock().expect("snapshot");
        match cmd.as_str() {
            "pause" => snapshot["running"] = serde_json::json!(false),
            "start" => snapshot["running"] = serde_json::json!(true),
            "reset" => {
                snapshot["running"] = serde_json::json!(false);
                let phase_seconds = snapshot["phase_seconds"].clone();
                snapshot["remaining_seconds"] = phase_seconds;
            }
            _ => {}
        }
        snapshot.clone()
    };
    let frame = door::answer_frame(id, Ok(answer));
    let _ = stream.write_all(format!("{frame}\n").as_bytes());
}

fn open_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    settings_json: &str,
) -> (gpui::Entity<Store>, &'a mut gpui::VisualTestContext) {
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service(name, settings_json)));
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    (store, cx)
}

/// (a) The rail reads Summary, Pomodoro, Tools, top to bottom.
#[gpui_kit::gpui::test]
fn the_rail_lists_pomodoro_between_summary_and_tools(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (_store, cx) = open_window(cx, "rail-order", "{}");
    draw(cx);
    draw(cx);

    let summary = cx.debug_bounds("rail-summary").expect("the Summary item");
    let pomodoro = cx.debug_bounds("rail-pomodoro").expect("the Pomodoro item");
    let tools = cx.debug_bounds("rail-tools").expect("the Tools item");
    assert!(
        summary.origin.y < pomodoro.origin.y,
        "Pomodoro {pomodoro:?} sits below Summary {summary:?}"
    );
    assert!(
        pomodoro.origin.y < tools.origin.y,
        "Pomodoro {pomodoro:?} sits above Tools {tools:?}"
    );
    assert_eq!(
        pomodoro.origin.x, summary.origin.x,
        "the rail is one column"
    );
}

/// (b) The page renders the live snapshot the door answers.
#[gpui_kit::gpui::test]
fn the_page_renders_the_live_snapshot(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let door = FakeDoor::start();
    // SAFETY: the env var is read once by the page when this window builds it;
    // ENV_LOCK keeps every other pomodoro test out of the window construction
    let original = std::env::var_os("WARP_STATION_POMODORO_DIR");
    unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", door.dir.path()) };
    let (_store, cx) = open_window(cx, "live-snapshot", "{}");
    match original {
        Some(value) => unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", value) },
        None => unsafe { std::env::remove_var("WARP_STATION_POMODORO_DIR") },
    }

    draw(cx);
    click(cx, "rail-pomodoro");
    draw(cx);
    draw(cx);

    cx.debug_bounds("pomodoro-page").expect("the page opens");
    cx.debug_bounds("pomodoro-status-Running")
        .expect("the live door reads as Running");
    cx.debug_bounds("pomodoro-remaining-24:03")
        .expect("the countdown shows 1443 s");
    cx.debug_bounds("pomodoro-phase-Work")
        .expect("the work phase chip");
    cx.debug_bounds("pomodoro-cycle-Cycle 2 of 4")
        .expect("the cycle dots and their count");
    cx.debug_bounds("pomodoro-today-5").expect("today's count");
    cx.debug_bounds("pomodoro-week-18")
        .expect("the 7-day count");
    assert!(
        door.commands().iter().any(|cmd| cmd == "status"),
        "the page polls the door: {:?}",
        door.commands()
    );
    assert!(
        cx.debug_bounds("toast-stack").is_none(),
        "a live door raises no toast"
    );
}

/// (c) Pause writes the door command and the answer moves the page.
#[gpui_kit::gpui::test]
fn the_pause_button_writes_the_door_command(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let door = FakeDoor::start();
    // SAFETY: see the live-snapshot probe
    let original = std::env::var_os("WARP_STATION_POMODORO_DIR");
    unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", door.dir.path()) };
    let (_store, cx) = open_window(cx, "pause-command", "{}");
    match original {
        Some(value) => unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", value) },
        None => unsafe { std::env::remove_var("WARP_STATION_POMODORO_DIR") },
    }

    draw(cx);
    click(cx, "rail-pomodoro");
    draw(cx);
    draw(cx);
    click(cx, "pomodoro-primary");
    draw(cx);
    draw(cx);

    assert!(
        door.commands().iter().any(|cmd| cmd == "pause"),
        "Pause reaches the door: {:?}",
        door.commands()
    );
    cx.debug_bounds("pomodoro-status-Paused")
        .expect("the answer moves the page to Paused");
}

/// (d) No door and no state file: "Not running" is the page's state, and the
/// closed app raises no error toast.
#[gpui_kit::gpui::test]
fn a_closed_app_reads_as_not_running_without_a_toast(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let empty = tempfile::tempdir().expect("temp dir");
    // SAFETY: see the live-snapshot probe
    let original = std::env::var_os("WARP_STATION_POMODORO_DIR");
    unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", empty.path()) };
    let (_store, cx) = open_window(cx, "not-running", "{}");
    match original {
        Some(value) => unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", value) },
        None => unsafe { std::env::remove_var("WARP_STATION_POMODORO_DIR") },
    }

    draw(cx);
    click(cx, "rail-pomodoro");
    draw(cx);
    draw(cx);

    cx.debug_bounds("pomodoro-status-Not running")
        .expect("a closed app is a state, not an error");
    cx.debug_bounds("pomodoro-empty").expect("the empty card");
    assert!(
        cx.debug_bounds("toast-stack").is_none(),
        "a closed app never toasts"
    );
}

/// A saved-off Pomodoro section hides its rail item and falls back to Repos.
#[gpui_kit::gpui::test]
fn a_disabled_pomodoro_hides_the_rail_item_and_falls_back(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // SAFETY: the page hook is read once when the window builds
    let original = std::env::var_os("WARP_STATION_PAGE");
    unsafe { std::env::set_var("WARP_STATION_PAGE", "pomodoro") };
    let (_store, cx) = open_window(
        cx,
        "disabled",
        r#"{"pomodoro": {"enable_pomodoro": false}}"#,
    );
    match original {
        Some(value) => unsafe { std::env::set_var("WARP_STATION_PAGE", value) },
        None => unsafe { std::env::remove_var("WARP_STATION_PAGE") },
    }

    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("rail-pomodoro").is_none(),
        "the saved-off section has no rail item"
    );
    assert!(
        cx.debug_bounds("repos-list-pane").is_some(),
        "the selected but disabled section falls back to Repos"
    );
}

/// With the door down, the snapshot the app left in `state.json` is shown as
/// the last state — and never as a live one.
#[gpui_kit::gpui::test]
fn a_stale_state_file_reads_as_not_running_with_its_last_seen(cx: &mut TestAppContext) {
    let _guard = env_guard();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let closed = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        closed.path().join("state.json"),
        serde_json::to_string(&snapshot_json(false, 301)).expect("snapshot"),
    )
    .expect("state file");
    // SAFETY: see the live-snapshot probe
    let original = std::env::var_os("WARP_STATION_POMODORO_DIR");
    unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", closed.path()) };
    let (_store, cx) = open_window(cx, "stale-file", "{}");
    match original {
        Some(value) => unsafe { std::env::set_var("WARP_STATION_POMODORO_DIR", value) },
        None => unsafe { std::env::remove_var("WARP_STATION_POMODORO_DIR") },
    }

    draw(cx);
    click(cx, "rail-pomodoro");
    draw(cx);
    draw(cx);

    cx.debug_bounds("pomodoro-status-Not running")
        .expect("a file without a door is never live");
    cx.debug_bounds("pomodoro-remaining-5:01")
        .expect("the file's last snapshot renders");
    cx.debug_bounds("pomodoro-last-seen")
        .expect("the page says when it last saw the app");
    assert!(
        cx.debug_bounds("toast-stack").is_none(),
        "a closed app never toasts"
    );
}
