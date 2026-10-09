//! The settings store: schema byte-compatible with the settings.json already
//! on disk (PascalCase keys, same defaults, enum-as-number sort mode) plus
//! the single-owner service with the legacy-key migration and the atomic
//! write.

use crate::jsonio;
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct AppSettings {
    #[serde(default)]
    pub nuget_local: NugetLocalSettings,
    #[serde(default)]
    pub repos: ReposSettings,
    #[serde(default)]
    pub clipboard_password: ClipboardPasswordSettings,
    #[serde(default)]
    pub snap_it: SnapItSettings,
    #[serde(default, rename = "OpenCode")]
    pub opencode: OpenCodeSettings,
    #[serde(default)]
    pub general: GeneralSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct NugetLocalSettings {
    pub enable_nuget: bool,
    pub watch_folder: Option<String>,
    pub nuget_package_filter: String,
    pub file_copy_delay_ms: u32,
    pub count_reset_interval_seconds: u32,
}

impl Default for NugetLocalSettings {
    fn default() -> Self {
        Self {
            enable_nuget: true,
            watch_folder: None,
            nuget_package_filter: "*.nupkg".into(),
            file_copy_delay_ms: 2000,
            count_reset_interval_seconds: 60,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ReposSettings {
    pub repo_scan_folders: Option<Vec<String>>,
    pub git_folder_pattern: String,
    pub solution_file_pattern: String,
    pub platform_folder_name: String,
    #[serde(rename = "VSCodeExecutable")]
    pub vscode_executable: String,
    #[serde(rename = "VSCodeProfile")]
    pub vscode_profile: String,
    pub terminal_executable: String,
    pub ide_executable: String,
    pub opencode_executable: String,
    pub zcode_executable: String,
    pub enable_terminal: bool,
    #[serde(rename = "EnableVSCode")]
    pub enable_vscode: bool,
    pub enable_visual_studio: bool,
    pub enable_zcode: bool,
    pub enable_git_hub: bool,
    pub git_hub_executable: String,
    pub enable_azure_dev_ops: bool,
    pub azure_dev_ops_pat: String,
    pub azure_dev_ops_url: String,
    pub excluded_folders: Option<Vec<String>>,
    pub sort_mode: RepoSortMode,
    pub max_scan_depth: i32,
    pub branch_name_prefix: String,
}

impl Default for ReposSettings {
    fn default() -> Self {
        Self {
            repo_scan_folders: None,
            git_folder_pattern: "*.git".into(),
            solution_file_pattern: "*.sln,*.slnx".into(),
            platform_folder_name: "platform".into(),
            vscode_executable: "code".into(),
            vscode_profile: String::new(),
            terminal_executable: "wt".into(),
            ide_executable: String::new(),
            opencode_executable: "opencode".into(),
            zcode_executable: "zcode".into(),
            enable_terminal: true,
            enable_vscode: true,
            enable_visual_studio: true,
            enable_zcode: true,
            enable_git_hub: true,
            git_hub_executable: "gh".into(),
            enable_azure_dev_ops: true,
            azure_dev_ops_pat: String::new(),
            azure_dev_ops_url: String::new(),
            excluded_folders: None,
            sort_mode: RepoSortMode::Name,
            max_scan_depth: 3,
            branch_name_prefix: "users/enginkirmaci".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RepoSortMode {
    /// Alphabetical by repo name (the historical default).
    #[default]
    Name = 0,
    /// Most recent commit first; repos without a known commit date last.
    LastActivity = 1,
    /// Most pending work first: changes, then to-push, then to-pull counts.
    Changes = 2,
    /// Most open GitHub pull requests first.
    PullRequests = 3,
    /// Most open GitHub issues first.
    Issues = 4,
}

// the settings file stores the enum as its number
impl Serialize for RepoSortMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for RepoSortMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match u8::deserialize(deserializer)? {
            1 => RepoSortMode::LastActivity,
            2 => RepoSortMode::Changes,
            3 => RepoSortMode::PullRequests,
            4 => RepoSortMode::Issues,
            _ => RepoSortMode::Name,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct OpenCodeSettings {
    #[serde(rename = "EnableOpenCode")]
    pub enable_opencode: bool,
    pub default_model: String,
    pub commit_model: Option<String>,
}

impl Default for OpenCodeSettings {
    fn default() -> Self {
        Self { enable_opencode: false, default_model: String::new(), commit_model: None }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ClipboardPasswordSettings {
    pub encrypted_password: Option<String>,
    pub enable_clipboard_password: bool,
}

impl Default for ClipboardPasswordSettings {
    fn default() -> Self {
        Self { encrypted_password: None, enable_clipboard_password: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SnapItSettings {
    /// Round-trip only: written by the Windows SnapIt companion, never read here.
    pub auto_start: bool,
}

impl Default for SnapItSettings {
    fn default() -> Self {
        Self { auto_start: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct GeneralSettings {
    pub start_minimized: bool,
    pub start_at_boot: bool,
    pub notes_store_path: String,
    pub show_tooltips: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            start_minimized: false,
            start_at_boot: false,
            notes_store_path: String::new(),
            show_tooltips: true,
        }
    }
}

/// Single owner of the settings graph: reads hand out full clones, writes are
/// serialized and persisted atomically. An unreadable file falls back to
/// defaults instead of failing startup.
pub struct SettingsService {
    path: PathBuf,
    state: Mutex<AppSettings>,
}

impl SettingsService {
    pub fn open() -> Self {
        Self::at(AppPaths::settings_file())
    }

    pub fn at(path: PathBuf) -> Self {
        let settings = match std::fs::read_to_string(&path) {
            Ok(json) => Self::parse(&json).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        };
        Self { path, state: Mutex::new(settings) }
    }

    fn parse(json: &str) -> anyhow::Result<AppSettings> {
        let migrated = migrate_legacy_enable_keys(json);
        Ok(serde_json::from_str(&migrated)?)
    }

    pub fn get(&self) -> AppSettings {
        self.state.lock().unwrap().clone()
    }

    /// Apply `mutate` to a clone of the current settings and persist the whole
    /// graph as one transition, so concurrent section updates stack instead of
    /// racing.
    pub fn update(&self, mutate: impl FnOnce(&mut AppSettings)) -> anyhow::Result<AppSettings> {
        let (updated, path) = {
            let mut guard = self.state.lock().unwrap();
            let mut next = guard.clone();
            mutate(&mut next);
            *guard = next.clone();
            (next, self.path.clone())
        };
        jsonio::write_json_atomically(&path, &updated)?;
        Ok(updated)
    }
}

/// Maps the legacy per-tool toggle keys to the uniform Enable<Tool> scheme,
/// each only when the new key is absent: Repos.ShowGitHubColumn →
/// Repos.EnableGitHub, Repos.ShowAzureDevOpsColumn → Repos.EnableAzureDevOps,
/// OpenCode.Enabled → OpenCode.EnableOpenCode, and the INVERTED
/// ClipboardPassword.HideFromGui → ClipboardPassword.EnableClipboardPassword.
fn migrate_legacy_enable_keys(json: &str) -> String {
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(json) else {
        return json.to_string();
    };
    let mut changed = false;
    let mut copy_bool = |section: &str, legacy: &str, new: &str, inverted: bool| {
        let Some(obj) = root.get_mut(section).and_then(|v| v.as_object_mut()) else {
            return;
        };
        let Some(legacy_value) = obj.get(legacy).and_then(|v| v.as_bool()) else {
            return;
        };
        if obj.contains_key(new) {
            return;
        }
        obj.insert(new.to_string(), serde_json::Value::Bool(if inverted {
            !legacy_value
        } else {
            legacy_value
        }));
        changed = true;
    };
    copy_bool("Repos", "ShowGitHubColumn", "EnableGitHub", false);
    copy_bool("Repos", "ShowAzureDevOpsColumn", "EnableAzureDevOps", false);
    copy_bool("OpenCode", "Enabled", "EnableOpenCode", false);
    copy_bool("ClipboardPassword", "HideFromGui", "EnableClipboardPassword", true);
    if changed {
        root.to_string()
    } else {
        json.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_csharp_file_shape() {
        let json = r#"{
            "Repos": {"RepoScanFolders": ["/a", "/b"], "SortMode": 2, "MaxScanDepth": 5,
                      "EnableGitHub": true, "AzureDevOpsPat": "pat"},
            "General": {"ShowTooltips": false},
            "OpenCode": {"EnableOpenCode": true, "DefaultModel": "x", "CommitModel": null}
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.repos.repo_scan_folders.as_deref(), Some(&["/a".to_string(), "/b".to_string()][..]));
        assert_eq!(s.repos.sort_mode, RepoSortMode::Changes);
        assert_eq!(s.repos.max_scan_depth, 5);
        assert!(!s.general.show_tooltips);
        assert!(s.opencode.enable_opencode);
        assert_eq!(s.opencode.commit_model, None);
        // defaults fill absent sections
        assert!(s.nuget_local.enable_nuget);
        assert_eq!(s.repos.branch_name_prefix, "users/enginkirmaci");
    }

    #[test]
    fn legacy_enable_keys_migrate_once() {
        let json = r#"{"OpenCode": {"Enabled": true}, "ClipboardPassword": {"HideFromGui": false}}"#;
        let s: AppSettings = serde_json::from_str(&migrate_legacy_enable_keys(json)).unwrap();
        assert!(s.opencode.enable_opencode);
        // inverted legacy key: HideFromGui=false → enabled
        assert!(s.clipboard_password.enable_clipboard_password);
        // an explicit new key wins over the legacy one
        let json2 = r#"{"OpenCode": {"Enabled": true, "EnableOpenCode": false}}"#;
        let s2: AppSettings = serde_json::from_str(&migrate_legacy_enable_keys(json2)).unwrap();
        assert!(!s2.opencode.enable_opencode);
    }

    #[test]
    fn round_trips_pascal_case() {
        let s = AppSettings::default();
        let json = serde_json::to_string_pretty(&s).unwrap();
        assert!(json.contains("\"RepoScanFolders\""));
        assert!(json.contains("\"MaxScanDepth\""));
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.repos.sort_mode, s.repos.sort_mode);
    }
}
