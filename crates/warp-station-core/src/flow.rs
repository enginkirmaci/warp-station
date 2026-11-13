//! The per-branch flow record behind the Changes strip: what the branch's
//! push turned into (a pull request, a merge, a run and its result). The file
//! is a regenerable cache (`cache/flow.json`), not a user setting: deleting it
//! loses only the strip's memory until the live passes answer again.
//!
//! Records are keyed by `folder\nbranch`; the merge target is kept inside the
//! pull record so the pipeline step can find the run the merge started.

use crate::activity::{ActivityRun, ActivitySource, RunState};
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use warpui_kit_platform::config::Store;

/// Records older than this are dropped on load and save: a branch left alone
/// for a month is not a flow the strip should still offer.
pub const FLOW_TTL_DAYS: i64 = 30;
/// A hard cap for a machine that opens hundreds of branches. Newest stay.
const FLOW_CAP: usize = 200;

/// One branch's remembered path through `Create PR → Merge → Pipeline →
/// Result`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default)]
pub struct FlowRecord {
    pub folder: String,
    pub branch: String,
    /// The last successful push or publish on this branch.
    pub pushed_at: Option<String>,
    pub pull: Option<FlowPull>,
    pub run: Option<FlowRun>,
    /// A multi-run merge gate's last summary (the watch-all switch).
    pub gate: Option<FlowGate>,
    pub updated_at: Option<String>,
}

impl FlowRecord {
    pub fn new(folder: impl Into<String>, branch: impl Into<String>) -> Self {
        Self {
            folder: folder.into(),
            branch: branch.into(),
            ..Default::default()
        }
    }

    pub fn is_merged(&self) -> bool {
        self.pull
            .as_ref()
            .is_some_and(|pull| pull.state == FlowPullState::Merged)
    }

    pub fn has_pull(&self) -> bool {
        self.pull.is_some()
    }

    /// A new push after a merged or closed pull request starts a new cycle.
    /// An open pull request keeps its record: the push only updates it.
    pub fn begin_push(&mut self, at: impl Into<String>) {
        self.pushed_at = Some(at.into());
        if self
            .pull
            .as_ref()
            .is_none_or(|pull| pull.state != FlowPullState::Open)
        {
            self.pull = None;
            self.run = None;
            self.gate = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowPullState {
    #[default]
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default)]
pub struct FlowPull {
    pub source: ActivitySource,
    pub number: u64,
    pub title: String,
    pub state: FlowPullState,
    /// The source branch (the branch the strip belongs to).
    pub head: String,
    /// The branch the pull request merges into; the merge's run lands here.
    pub target: String,
    pub url: Option<String>,
    /// When the merge completed, ISO; the pipeline step's lower bound.
    pub merged_at: Option<String>,
}

impl FlowPull {
    pub fn is_merged(&self) -> bool {
        self.state == FlowPullState::Merged
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct FlowRun {
    pub source: ActivitySource,
    pub id: u64,
    pub name: String,
    pub label: String,
    pub state: RunState,
    /// The run's own branch (the merge target), not the strip's branch.
    pub branch: Option<String>,
    pub url: Option<String>,
    pub at: Option<String>,
}

impl FlowRun {
    pub fn of(run: &ActivityRun) -> Self {
        Self {
            source: run.source,
            id: run.id,
            name: run.name.clone(),
            label: run.label.clone(),
            state: run.state,
            branch: run.branch.clone(),
            url: run.url.clone(),
            at: run.at.clone(),
        }
    }

    /// The row the Pipelines pane opens when the strip's Result step is
    /// clicked after a restart, before the run list has answered.
    pub fn to_activity_run(&self) -> ActivityRun {
        ActivityRun {
            source: self.source,
            id: self.id,
            name: self.name.clone(),
            label: self.label.clone(),
            branch: self.branch.clone(),
            state: self.state,
            url: self.url.clone(),
            at: self.at.clone(),
            ..Default::default()
        }
    }

    pub fn finished(&self) -> bool {
        matches!(
            self.state,
            RunState::Succeeded | RunState::Failed | RunState::Skipped
        )
    }

    pub fn passed(&self) -> bool {
        self.state == RunState::Succeeded
    }
}

/// The merge gate's last tally across every run the merge started on the
/// target branch. Written by the watch-all setting; the single-run flow leaves
/// it empty.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default)]
pub struct FlowGate {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub running: usize,
    /// All runs known when the gate closed (or the watch gave up).
    pub settled: bool,
    pub at: Option<String>,
}

impl FlowGate {
    /// The strip's one-line result: "2 of 3 passed" while it counts, and a
    /// failure leads when there is one.
    pub fn summary(&self) -> String {
        if self.running > 0 {
            return format!("{} of {} running", self.running, self.total);
        }
        if self.failed > 0 {
            format!(
                "{} of {} passed · {} failed",
                self.passed, self.total, self.failed
            )
        } else {
            format!("{} of {} passed", self.passed, self.total)
        }
    }

    pub fn all_passed(&self) -> bool {
        self.total > 0 && self.passed == self.total
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", default)]
pub struct FlowCacheFile {
    pub flows: BTreeMap<String, FlowRecord>,
}

/// The map key for one branch. A unit separator cannot occur in a path or a
/// branch name, so folder and branch cannot collide.
pub fn key(folder: &str, branch: &str) -> String {
    format!("{folder}\u{1f}{branch}")
}

pub fn cache_path() -> PathBuf {
    AppPaths::flow_cache_file()
}

pub fn load_cache() -> FlowCacheFile {
    load_from(&cache_path())
}

pub fn save_cache(cache: &FlowCacheFile) -> anyhow::Result<()> {
    save_to(&cache_path(), cache)
}

pub fn load_from(path: &Path) -> FlowCacheFile {
    let mut cache = Store::<FlowCacheFile>::new(path.to_path_buf())
        .load()
        .unwrap_or_default();
    cache.prune();
    cache
}

pub fn save_to(path: &Path, cache: &FlowCacheFile) -> anyhow::Result<()> {
    let mut cache = cache.clone();
    cache.prune();
    Store::<FlowCacheFile>::new(path.to_path_buf())
        .save(&cache)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

impl FlowCacheFile {
    /// Drops month-old records and caps the map at the newest [FLOW_CAP].
    pub fn prune(&mut self) {
        let cutoff = chrono::Local::now() - chrono::Duration::days(FLOW_TTL_DAYS);
        self.flows.retain(|_, record| {
            record
                .updated_at
                .as_deref()
                .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
                .map(|at| at.with_timezone(&chrono::Local) >= cutoff)
                .unwrap_or(false)
        });
        if self.flows.len() > FLOW_CAP {
            let mut newest: Vec<(String, Option<String>)> = self
                .flows
                .iter()
                .map(|(key, record)| (key.clone(), record.updated_at.clone()))
                .collect();
            newest.sort_by(|a, b| b.1.cmp(&a.1));
            let keep: std::collections::HashSet<String> = newest
                .into_iter()
                .take(FLOW_CAP)
                .map(|(key, _)| key)
                .collect();
            self.flows.retain(|key, _| keep.contains(key));
        }
    }

    pub fn get(&self, folder: &str, branch: &str) -> Option<&FlowRecord> {
        self.flows.get(&key(folder, branch))
    }

    pub fn record(
        &mut self,
        folder: &str,
        branch: &str,
        now: impl Into<String>,
    ) -> &mut FlowRecord {
        let record = self
            .flows
            .entry(key(folder, branch))
            .or_insert_with(|| FlowRecord::new(folder, branch));
        record.updated_at = Some(now.into());
        record
    }

    pub fn remove(&mut self, folder: &str, branch: &str) -> bool {
        self.flows.remove(&key(folder, branch)).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "warp-station-flow-{name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn a_record_round_trips_through_the_cache_file() {
        let path = temp_path("roundtrip");
        let mut cache = FlowCacheFile::default();
        let record = cache.record("/work/alpha", "feat/flow", "2026-11-13T10:00:00+03:00");
        record.pushed_at = Some("2026-11-13T09:50:00+03:00".into());
        record.pull = Some(FlowPull {
            source: ActivitySource::GitHub,
            number: 42,
            title: "Land the flow".into(),
            state: FlowPullState::Merged,
            head: "feat/flow".into(),
            target: "main".into(),
            url: Some("https://github.com/o/alpha/pull/42".into()),
            merged_at: Some("2026-11-13T10:02:00+03:00".into()),
        });
        record.run = Some(FlowRun {
            source: ActivitySource::GitHub,
            id: 9001,
            name: "kits".into(),
            label: "#1284".into(),
            state: RunState::Succeeded,
            branch: Some("main".into()),
            url: None,
            at: Some("2026-11-13T10:00:00+03:00".into()),
        });
        save_to(&path, &cache).expect("save");
        let back = load_from(&path);
        assert_eq!(back, cache);
        let record = back.get("/work/alpha", "feat/flow").expect("record");
        assert!(record.is_merged());
        assert!(record.run.as_ref().is_some_and(FlowRun::passed));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_new_push_keeps_an_open_pull_and_clears_a_merged_one() {
        let mut record = FlowRecord::new("/work/alpha", "feat/flow");
        record.pull = Some(FlowPull {
            source: ActivitySource::GitHub,
            number: 1,
            title: "open".into(),
            state: FlowPullState::Open,
            head: "feat/flow".into(),
            target: "main".into(),
            url: None,
            merged_at: None,
        });
        record.begin_push("2026-11-13T10:00:00+03:00");
        assert!(
            record.has_pull(),
            "an open pull request is updated, not lost"
        );
        record.pull.as_mut().expect("pull").state = FlowPullState::Merged;
        record.begin_push("2026-11-13T10:05:00+03:00");
        assert!(
            !record.has_pull(),
            "a merged pull request starts a new cycle"
        );
    }

    #[test]
    fn old_records_are_pruned_and_the_newest_survive_the_cap() {
        let mut cache = FlowCacheFile::default();
        let old = (chrono::Local::now() - chrono::Duration::days(FLOW_TTL_DAYS + 1)).to_rfc3339();
        cache.flows.insert(
            key("/work/old", "main"),
            FlowRecord {
                updated_at: Some(old),
                ..FlowRecord::new("/work/old", "main")
            },
        );
        cache.record("/work/new", "feat/flow", chrono::Local::now().to_rfc3339());
        cache.prune();
        assert_eq!(cache.flows.len(), 1);
        assert!(cache.get("/work/new", "feat/flow").is_some());
    }

    #[test]
    fn the_gate_summary_names_the_count_and_a_failure() {
        let mut gate = FlowGate {
            total: 3,
            passed: 2,
            failed: 0,
            skipped: 0,
            running: 1,
            settled: false,
            at: None,
        };
        assert_eq!(gate.summary(), "1 of 3 running");
        gate.running = 0;
        gate.failed = 1;
        assert_eq!(gate.summary(), "2 of 3 passed · 1 failed");
        assert!(!gate.all_passed());
    }
}
