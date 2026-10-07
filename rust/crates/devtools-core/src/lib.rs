//! Headless services port of Tools.Library: settings, repo scanning, git reads
//! and mutations, GitHub/Azure DevOps activity, opencode integration, notes.
//! No gpui dependencies — everything here must be testable without a display.

pub mod azure;
pub mod exe;
pub mod git;
pub mod github;
pub mod jsonio;
pub mod model;
pub mod paths;
pub mod process;
pub mod refresh;
pub mod rel_time;
pub mod repo_store;
pub mod scan;
pub mod terminal;
pub mod settings;
