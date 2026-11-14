//! The shortcut sheet's content: every key Warp Station answers to, grouped
//! by the surface that owns it.
//!
//! The rows live here once. The launcher rows are generated from
//! [`crate::ui::launchers::launchers`] so the `Shift` letter a menu keycap
//! shows and the one the sheet prints can never disagree; the fixed rows are
//! pinned by the tests against the keys the handlers match.

use crate::ui::launchers;
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::AppSettings;
use warpui_kit_core::shortcuts::Group;

fn row(
    keys: impl Into<gpui::SharedString>,
    label: impl Into<gpui::SharedString>,
) -> (gpui::SharedString, gpui::SharedString) {
    (keys.into(), label.into())
}

/// What the sheet describes before a repository is selected: the launchers
/// that need no repository state, and no solution path.
fn placeholder_repo() -> RepoEntry {
    RepoEntry {
        name: String::new(),
        folder_path: std::path::PathBuf::new(),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: None,
        git_modified_count: 0,
        git_to_push_count: 0,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: false,
        git_branch_published: false,
        git_status_loaded: false,
        github_pr_count: 0,
        github_issue_count: 0,
        azure_pull_count: 0,
        github_web_url: None,
        azure_work_item_count: 0,
    }
}

/// The whole sheet for this settings/repository pair. A repository is
/// optional: the dialog opens from any section, and a placeholder keeps the
/// launcher chords listed before a row is picked.
pub fn groups(settings: &AppSettings, repo: Option<&RepoEntry>) -> Vec<Group> {
    let window = Group::new(
        "Window",
        vec![
            row("Ctrl K", "Focus the search field"),
            row("? / Ctrl /", "Show this shortcut sheet"),
            row("Ctrl \\", "Show or hide the repository list"),
            row(
                "Esc",
                "Close the topmost layer, then leave the section, then clear the search",
            ),
            row("Ctrl Shift V", "Copy the stored Clipboard Password"),
            row("Ctrl C", "Copy the selected read-only text"),
        ],
    );
    let tabs = Group::new(
        "Repository tabs",
        vec![
            row(
                "Ctrl T / Ctrl Enter",
                "Open the selected repository in a tab",
            ),
            row("Ctrl W", "Close the active tab"),
            row("Ctrl Tab", "Next tab"),
            row("Ctrl Shift Tab", "Previous tab"),
            row("Ctrl 1…9", "Jump to a tab"),
        ],
    );
    let lists = Group::new(
        "Lists (any focused list)",
        vec![
            row("↑ / ↓", "Move the cursor"),
            row("Home / End", "First / last row"),
            row("PageUp / PageDown", "Move a page"),
            row("← / →", "Focus the list on the left / right"),
            row("Tab / Shift Tab", "Same, when a list is on that side"),
        ],
    );
    let mut repository_rows = vec![
        row("Enter", "Open the selected repository in a tab"),
        row("Alt F", "Favorite or unfavorite the selected repository"),
        row("Alt A", "Archive or restore the selected repository"),
        row("Alt C", "Open the Changes tab"),
    ];
    // The launcher rows are the same list the menu draws. Before a repository
    // is picked the sheet still describes the chords, using a placeholder with
    // no solution (Visual Studio needs one, so it stays out).
    let sheet_repo = repo.cloned().unwrap_or_else(placeholder_repo);
    for launcher in launchers::launchers(settings, &sheet_repo) {
        repository_rows.push(row(
            format!("Shift {}", launcher.key),
            launcher.label.to_string(),
        ));
    }
    let repositories = Group::new("Repositories", repository_rows);
    let changes = Group::new(
        "Changes",
        vec![row(
            "Ctrl Enter",
            "Commit the staged changes, from the message box",
        )],
    );
    let merge = Group::new(
        "Merge conflicts",
        vec![
            row("F7 / Shift F7", "Next / previous conflict"),
            row("F10 / F11", "Take all incoming / local"),
            row("Ctrl Enter", "Accept the merge, from the merge editor"),
            row("Esc", "Leave the merge editor"),
        ],
    );
    vec![window, tabs, lists, repositories, changes, merge]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn repo() -> RepoEntry {
        RepoEntry {
            name: "alpha".into(),
            folder_path: PathBuf::from("/probe/alpha"),
            solution_path: None,
            tags: Vec::new(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: true,
            git_branch_published: true,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
            azure_pull_count: 0,
            github_web_url: None,
            azure_work_item_count: 0,
        }
    }

    fn all_enabled() -> AppSettings {
        let mut settings = AppSettings::default();
        settings.repos.enable_vscode = true;
        settings.repos.enable_terminal = true;
        settings.opencode.enable_opencode = true;
        settings.deepseek.enable_deepseek = true;
        settings.warp_agent.enable_warp_agent = true;
        settings.warp_notes.enable_warp_notes = true;
        settings
    }

    fn keys(groups: &[Group], title: &str) -> Vec<String> {
        groups
            .iter()
            .find(|group| group.title.as_ref() == title)
            .map(|group| {
                group
                    .rows
                    .iter()
                    .map(|(keys, _)| keys.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn every_launcher_is_listed_with_its_shift_key() {
        let groups = groups(&all_enabled(), Some(&repo()));
        let repositories = keys(&groups, "Repositories");
        let launchers = launchers::launchers(&all_enabled(), &repo());
        for launcher in &launchers {
            assert!(
                repositories
                    .iter()
                    .any(|key| key == &format!("Shift {}", launcher.key)),
                "{} is listed",
                launcher.label
            );
        }
        assert_eq!(repositories.len(), 4 + launchers.len());
    }

    #[test]
    fn the_fixed_repository_keys_are_listed() {
        let repositories = keys(&groups(&all_enabled(), Some(&repo())), "Repositories");
        for key in ["Enter", "Alt F", "Alt A", "Alt C"] {
            assert!(repositories.iter().any(|row| row == key), "{key}");
        }
    }

    #[test]
    fn no_group_repeats_a_key() {
        let groups = groups(&all_enabled(), Some(&repo()));
        for group in &groups {
            let mut seen = HashSet::new();
            for (keys, _) in &group.rows {
                assert!(
                    seen.insert(keys.to_string()),
                    "{} repeats {keys}",
                    group.title
                );
            }
        }
    }

    #[test]
    fn without_a_repository_the_chords_are_still_listed() {
        let groups = groups(&all_enabled(), None);
        let repositories = keys(&groups, "Repositories");
        assert!(
            repositories.iter().any(|key| key == "Shift C"),
            "VS Code is described before a row is picked"
        );
        assert!(
            !repositories.iter().any(|key| key == "Shift V"),
            "Visual Studio needs a solution, so it stays out"
        );
    }
}
