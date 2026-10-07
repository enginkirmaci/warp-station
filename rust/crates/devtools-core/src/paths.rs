//! Global per-user locations. Everything lives under ~/.devtools — the same
//! root the C# app used, so both builds coexist on one data set.

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

fn home() -> PathBuf {
    // $HOME on unix; Windows only defines USERPROFILE
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
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
