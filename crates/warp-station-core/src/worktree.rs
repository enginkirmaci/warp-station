//! Worktrees of a repository: the kit owns the rules (where they live, how a
//! branch becomes a folder, the local exclude entry), and this module gives
//! the app the one shape its tab speaks — a folder in, rows out — while the git
//! calls stay off the UI thread.

use crate::git::read;
use crate::model::WorktreeRow;
use std::path::Path;

/// The repository's linked worktrees, its own checkout excluded, each with its
/// uncommitted count and its distance from upstream.
pub fn rows(folder: &Path) -> Result<Vec<WorktreeRow>, String> {
    let entries = warpui_kit_worktree::list(folder).map_err(|err| format!("{err:#}"))?;
    Ok(entries
        .into_iter()
        .filter(|entry| !entry.main)
        .map(|entry| {
            // the same probe the repository row runs, so a worktree's numbers
            // read the same way its repository's do
            let probe = read::probe(&entry.path);
            WorktreeRow {
                path: entry.path.to_string_lossy().to_string(),
                branch: entry.label(),
                dirty: probe.as_ref().map_or(0, |probe| probe.modified_count),
                ahead: probe.as_ref().map_or(0, |probe| probe.ahead_count),
                behind: probe.as_ref().map_or(0, |probe| probe.behind_count),
            }
        })
        .collect())
}

/// Removes one checkout's folder and its entry. A dirty worktree refuses until
/// `force` is set; its branch always survives, and a chat that lived there is
/// not touched.
pub fn remove(folder: &Path, path: &Path, force: bool) -> Result<(), String> {
    warpui_kit_worktree::remove(folder, path, force).map_err(|err| format!("{err:#}"))
}

/// Drops the entries whose folders are gone, and reports how many went.
pub fn prune(folder: &Path) -> Result<usize, String> {
    warpui_kit_worktree::prune(folder).map_err(|err| format!("{err:#}"))
}
