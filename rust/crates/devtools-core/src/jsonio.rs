//! Shared JSON plumbing: indented writes and the atomic replace (unique temp
//! file + rename in the target directory), so a crash mid-write never leaves a
//! truncated config behind.

use std::path::Path;

pub fn read_json(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Pretty-serialize `value` and replace `path` atomically.
pub fn write_json_atomically<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    write_atomically(path, &json)
}

/// Replace `path` with `contents` atomically: write a uniquely named temp file
/// next to the target, then rename over it (POSIX rename / Win32 replace —
/// atomic on the same volume).
pub fn write_atomically(path: &Path, contents: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let tmp = path.with_file_name(format!(
        "{file_name}.{}.tmp",
        uuid_suffix()
    ));
    std::fs::write(&tmp, contents).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn uuid_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{}", std::process::id())
}
