//! The workbench's resizable columns: one name per list, the design's width,
//! and the bounds a drag may leave it at.
//!
//! The width itself lives on the store ([`crate::store::Store::pane_width`]);
//! this module only answers what a column is called and how far it may move, so
//! every view clamps the same way. The page list keeps its older
//! `repos.list_width` home; the workbench columns share
//! `settings.layout.pane_widths`.

use warpui_kit_core::shell;

/// One list column the user can drag.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Pane {
    /// The list beside a page's detail: Summaries, Settings, Tools, About and
    /// the merge editor's files.
    PageList,
    /// The Pull requests, Issues and Pipelines work list.
    WorkList,
    /// The changed-file rail beside a diff (the pull request's and the Azure
    /// pull request's Files tabs).
    FilesRail,
    /// The pull request's day-grouped commit list.
    CommitsList,
    /// The selected commit's own files, inside the Commits tab.
    CommitFiles,
    /// The Changes tab's changed-file column.
    ChangesFiles,
    /// The Changes tab's History commit column.
    HistoryList,
    /// The History commit's own files rail.
    CommitRail,
}

impl Pane {
    /// Every column, for the tests that check the bounds.
    pub const ALL: [Pane; 8] = [
        Pane::PageList,
        Pane::WorkList,
        Pane::FilesRail,
        Pane::CommitsList,
        Pane::CommitFiles,
        Pane::ChangesFiles,
        Pane::HistoryList,
        Pane::CommitRail,
    ];

    /// The settings key and the drag's name.
    pub fn key(self) -> &'static str {
        match self {
            Pane::PageList => "page_list",
            Pane::WorkList => "work_list",
            Pane::FilesRail => "files_rail",
            Pane::CommitsList => "commits_list",
            Pane::CommitFiles => "commit_files",
            Pane::ChangesFiles => "changes_files",
            Pane::HistoryList => "history_list",
            Pane::CommitRail => "commit_rail",
        }
    }

    /// The debug selector of this column's drag bar. The page list keeps the
    /// name its probe already knows.
    pub fn selector(self) -> &'static str {
        match self {
            Pane::PageList => "list-resize",
            Pane::WorkList => "resize-work-list",
            Pane::FilesRail => "resize-files-rail",
            Pane::CommitsList => "resize-commits-list",
            Pane::CommitFiles => "resize-commit-files",
            Pane::ChangesFiles => "resize-changes-files",
            Pane::HistoryList => "resize-history-list",
            Pane::CommitRail => "resize-commit-rail",
        }
    }

    /// The design's width, before the text scale.
    pub fn default_width(self) -> f32 {
        match self {
            Pane::PageList => shell::LIST_WIDTH,
            Pane::WorkList => 320.,
            Pane::FilesRail => 240.,
            Pane::CommitsList => 220.,
            Pane::CommitFiles => 200.,
            Pane::ChangesFiles => 320.,
            Pane::HistoryList => 320.,
            Pane::CommitRail => 240.,
        }
    }

    /// The narrowest the column goes: the head and a row's leading columns
    /// still read.
    pub fn min_width(self) -> f32 {
        match self {
            Pane::PageList => shell::LIST_MIN_WIDTH,
            Pane::WorkList => 240.,
            Pane::FilesRail => 160.,
            Pane::CommitsList => 180.,
            Pane::CommitFiles => 150.,
            Pane::ChangesFiles => 240.,
            Pane::HistoryList => 240.,
            Pane::CommitRail => 180.,
        }
    }

    /// The widest the column goes: past it the diff has no room left.
    pub fn max_width(self) -> f32 {
        match self {
            Pane::PageList => shell::LIST_MAX_WIDTH,
            Pane::WorkList => 520.,
            Pane::FilesRail => 520.,
            Pane::CommitsList => 460.,
            Pane::CommitFiles => 460.,
            Pane::ChangesFiles => 560.,
            Pane::HistoryList => 560.,
            Pane::CommitRail => 480.,
        }
    }

    /// `width` inside this column's bounds, at the live text size.
    pub fn clamp(self, width: f32) -> f32 {
        width.clamp(
            shell::scaled(self.min_width()),
            shell::scaled(self.max_width()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_column_starts_inside_its_own_bounds() {
        for pane in Pane::ALL {
            let width = pane.clamp(shell::scaled(pane.default_width()));
            assert_eq!(width, shell::scaled(pane.default_width()), "{pane:?}");
            assert!(width >= shell::scaled(pane.min_width()), "{pane:?}");
            assert!(width <= shell::scaled(pane.max_width()), "{pane:?}");
        }
    }

    #[test]
    fn a_drag_past_either_edge_stops_at_the_bound() {
        for pane in Pane::ALL {
            assert_eq!(pane.clamp(0.), shell::scaled(pane.min_width()), "{pane:?}");
            assert_eq!(
                pane.clamp(10_000.),
                shell::scaled(pane.max_width()),
                "{pane:?}"
            );
        }
    }

    #[test]
    fn every_column_has_its_own_settings_key_and_selector() {
        let mut keys = std::collections::BTreeSet::new();
        let mut selectors = std::collections::BTreeSet::new();
        for pane in Pane::ALL {
            assert!(keys.insert(pane.key()), "{pane:?} repeats a key");
            assert!(
                selectors.insert(pane.selector()),
                "{pane:?} repeats a selector"
            );
        }
    }
}
