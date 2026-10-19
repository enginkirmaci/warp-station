//! Global per-user locations. Everything lives under ~/.devtools — one data
//! set shared across app versions; never relocate the root or change the
//! on-disk formats.

use std::path::PathBuf;

pub struct AppPaths;

impl AppPaths {
    pub fn user_data_root() -> PathBuf {
        home().join(".devtools")
    }
    pub fn logs_dir() -> PathBuf {
        Self::user_data_root().join("logs")
    }
    pub fn settings_file() -> PathBuf {
        Self::user_data_root().join("settings.json")
    }
    pub fn repos_cache_file() -> PathBuf {
        Self::user_data_root().join("repos.cache.json")
    }
    pub fn notes_dir() -> PathBuf {
        Self::user_data_root().join("notes")
    }
    pub fn opencode_dir() -> PathBuf {
        Self::user_data_root().join("opencode")
    }
}

/// $HOME on unix; USERPROFILE first on Windows — a POSIX-style $HOME leaked
/// from a Git Bash parent shell is not a valid Win32 path.
pub(crate) fn home() -> PathBuf {
    let keys: [&str; 2] = if cfg!(windows) { ["USERPROFILE", "HOME"] } else { ["HOME", "USERPROFILE"] };
    keys.iter()
        .find_map(|key| std::env::var_os(key).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Folder-text identity: case-insensitive on Windows (the filesystem is),
/// exact on case-sensitive platforms.
pub fn same_folder_text(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    {
        a.eq_ignore_ascii_case(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_dot_devtools_under_home() {
        let p = AppPaths::user_data_root();
        assert!(p.ends_with(".devtools"));
        assert!(AppPaths::settings_file().ends_with("settings.json"));
        assert!(AppPaths::repos_cache_file().ends_with("repos.cache.json"));
    }
}
