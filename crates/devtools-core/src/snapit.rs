//! Start/stop the SnapIt companion that snaps dragged windows into zone
//! layouts; it watches this process (--parent) and exits when DevTools dies.
//! The binary is the packaged Tools.SnapIt.exe next to the app unless a
//! configured executable (settings, like the warp-agent one) resolves.

#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::process::{Child, Command};
#[cfg(windows)]
use std::sync::Mutex;

#[cfg(windows)]
const COMPANION_EXE: &str = "Tools.SnapIt.exe";

#[cfg(windows)]
static CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// The configured executable when set (absolute path or bare name), else the
/// packaged companion next to the app.
#[cfg(windows)]
fn companion_path(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return crate::exe::locate(configured);
    }
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let path = dir.join(COMPANION_EXE);
    path.is_file().then_some(path)
}

#[cfg(windows)]
pub fn is_running() -> bool {
    let Ok(mut slot) = CHILD.lock() else {
        return false;
    };
    let Some(child) = slot.as_mut() else {
        return false;
    };
    matches!(child.try_wait(), Ok(None))
}

#[cfg(windows)]
pub fn start(configured: &str) -> Result<(), String> {
    let mut slot = CHILD
        .lock()
        .map_err(|_| "SnapIt state poisoned".to_string())?;
    if let Some(child) = slot.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(());
        }
    }
    let configured = configured.trim();
    let path = companion_path(configured).ok_or_else(|| {
        if configured.is_empty() {
            format!("{COMPANION_EXE} not found next to the app")
        } else {
            format!("SnapIt executable not found: {configured}")
        }
    })?;

    let mut command = Command::new(&path);
    command.arg("--parent").arg(std::process::id().to_string());
    // no stray console window behind the companion
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);

    *slot = Some(
        command
            .spawn()
            .map_err(|err| format!("could not start SnapIt: {err}"))?,
    );
    Ok(())
}

#[cfg(windows)]
pub fn stop() -> Result<(), String> {
    let mut slot = CHILD
        .lock()
        .map_err(|_| "SnapIt state poisoned".to_string())?;
    if let Some(mut child) = slot.take() {
        child
            .kill()
            .map_err(|err| format!("could not stop SnapIt: {err}"))?;
        let _ = child.wait();
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn is_running() -> bool {
    false
}

#[cfg(not(windows))]
pub fn start(_configured: &str) -> Result<(), String> {
    Err("SnapIt is Windows-only".to_string())
}

#[cfg(not(windows))]
pub fn stop() -> Result<(), String> {
    Ok(())
}
