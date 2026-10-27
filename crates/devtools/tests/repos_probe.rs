//! Headless probe for the repos table: a real Store + MainWindow pair drawn
//! in a test window, driven through the kit's own selection path — the row
//! click handler, the TableEvent subscription and the store's selection all
//! verified without a display.

use devtools::store::Store;
use devtools::ui::window::MainWindow;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::TestAppContext;
use std::path::PathBuf;

fn entry(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 1,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
    }
}

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &["favorites"]),
            entry("beta", "/probe/beta", &[]),
            entry("gamma", "/probe/gamma", &[]),
        ]);
        store
    })
}

#[gpui::test]
fn row_selection_reaches_the_store(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // the row-height wrapper fills the two-line row, rows advance by one row
    // height, and the name cell sits centered inside its row
    let wrap0 = cx.debug_bounds("repos-td-wrap-0").expect("row 0 renders");
    let wrap1 = cx.debug_bounds("repos-td-wrap-1").expect("row 1 renders");
    assert!(
        (wrap0.size.height - gpui::px(48.)).abs() < gpui::px(2.),
        "cells fill the 48px row (got {:?})",
        wrap0.size.height
    );
    let row_pitch = wrap1.origin.y - wrap0.origin.y;
    assert!(
        (row_pitch - gpui::px(48.)).abs() < gpui::px(2.),
        "rows advance by the 48px row height (got {row_pitch})",
    );
    let cell = cx
        .debug_bounds("repos-name-cell-0")
        .expect("name cell renders");
    let cell_mid = cell.origin.y + cell.size.height / 2.;
    let wrap_mid = wrap0.origin.y + wrap0.size.height / 2.;
    assert!(
        (cell_mid - wrap_mid).abs() < gpui::px(2.),
        "the name cell centers in its row (cell mid {cell_mid} vs row mid {wrap_mid})"
    );

    // the table's own selected-row API drives the same path a click does
    let selected_before = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    assert!(selected_before.is_none());

    // click into the first row through the real event pipeline
    cx.simulate_mouse_down(gpui::point(px_col(), px_row()), gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(gpui::point(px_col(), px_row()), gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let selected = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    assert_eq!(
        selected.as_deref(),
        Some("/probe/alpha"),
        "clicking the first row should select alpha (got {selected:?})"
    );
}

/// A dialog is a modal over the whole workspace: while it is open the page
/// takes the body height (the bar would otherwise paint over the dialog's
/// footer) and Escape brings the bar back.
#[gpui::test]
fn add_dialog_covers_the_workspace_and_escape_restores_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx)
    });
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("bar-root").is_some(), "bar starts open");

    let add = cx
        .debug_bounds("add-repo-button")
        .expect("add-repository button in the toolbar");
    cx.simulate_click(
        gpui::point(add.origin.x + add.size.width / 2., add.origin.y + add.size.height / 2.),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "dialog is open",
    );
    assert!(
        cx.debug_bounds("bar-root").is_none(),
        "the bar must yield the body height to the dialog",
    );

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "Escape closes the dialog",
    );
    assert!(
        cx.debug_bounds("bar-root").is_some(),
        "the bar comes back after the dialog closes",
    );
}

#[gpui::test]
fn selection_survives_and_closes_via_the_store(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/beta".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(cx.read_entity(&store, |s, _| s.selected_repo.clone()).as_deref(), Some("/probe/beta"));
    assert!(cx.read_entity(&store, |s, _| s.selected_entry().is_some()));

    cx.update_entity(&store, |s: &mut Store, cx| s.select_repo(None, cx));
    assert!(cx.read_entity(&store, |s, _| s.selected_repo.is_none()));
}

/// App alerts surface as toasts in the window's bottom-right corner, not as
/// inline header text: a store push renders a severity card, and its close
/// button removes exactly that card.
#[gpui::test]
fn toasts_render_stack_and_dismiss(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.notify_error("Pull failed for warp-agent: cannot pull with rebase");
        s.notify_success("Fetched alpha");
        cx.notify();
    });

    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("toast-stack").is_some(), "the toast stack renders");
    assert!(cx.debug_bounds("toast-0").is_some(), "the error toast renders");
    assert!(cx.debug_bounds("toast-1").is_some(), "the success toast renders");

    let close = cx
        .debug_bounds("toast-close-1")
        .expect("each toast carries a close button");
    cx.simulate_click(close.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("toast-1").is_none(), "the dismissed toast is gone");
    assert!(cx.debug_bounds("toast-0").is_some(), "the sibling toast stays");
}

/// The branch dropdown follows the kebab menu's dismissal contract: Escape
/// closes it, a click anywhere outside the card closes it, and picking a
/// branch closes it — only the toggle button reopens it.
#[gpui::test]
fn branch_menu_closes_on_escape_outside_and_select(cx: &mut TestAppContext) {
    use devtools_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx);
        // the branch selector lives on the Changes tab
        s.set_tab(devtools::store::BottomBarTab::Changes, cx);
        s.branches = vec![
            GitBranchRef { name: "main".into(), kind: GitBranchKind::Local },
            GitBranchRef { name: "feature".into(), kind: GitBranchKind::Local },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let open_menu = |cx: &mut gpui::VisualTestContext| {
        let button = cx
            .debug_bounds("branch-button")
            .expect("the branch selector renders in the rail");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| drop(window.draw(cx)));
    };

    open_menu(cx);
    assert!(cx.debug_bounds("branch-menu").is_some(), "the dropdown opens");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "Escape closes the dropdown"
    );

    // a click on the tab strip is outside the card: it dismisses the menu
    open_menu(cx);
    let bar = cx.debug_bounds("bar-root").expect("the bar renders");
    cx.simulate_click(
        gpui::point(bar.origin.x + gpui::px(60.), bar.origin.y + gpui::px(30.)),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "clicking outside the card closes the dropdown"
    );

    // picking a branch checks it out and closes the dropdown
    open_menu(cx);
    let row = cx
        .debug_bounds("branch-row-feature")
        .expect("the filtered branch rows render");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "picking a branch closes the dropdown"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.git_busy),
        "the checkout went out"
    );

    // the toggle button reopens after all of the above
    open_menu(cx);
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown reopens from the button"
    );
}

fn px_col() -> gpui::Pixels {
    gpui::px(120.)
}

fn px_row() -> gpui::Pixels {
    // titlebar 38 + two 46px toolbar rows + 28px column header + row center
    // (48px rows)
    gpui::px(182.)
}

fn entry_with_branch(name: &str, path: &str, branch: &str) -> RepoEntry {
    let mut repo = entry(name, path, &[]);
    repo.git_branch_name = Some(branch.into());
    repo
}

/// The columns are sized dynamically: the branch column hugs the widest
/// branch in the projection and the name column absorbs whatever is left, so
/// the widths always fill the table's available width exactly.
#[gpui::test]
fn columns_distribute_the_available_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        s.repos.push(entry_with_branch("delta", "/probe/delta", "release/feature-x"));
        s.projected.push(3); // the delegate reads the projection, not repos
    });

    cx.update(|cx| {
        let available = gpui::px(1100.);
        let delegate = devtools::ui::repos::ReposDelegate {
            store: store.clone(),
            available_width: std::rc::Rc::new(std::cell::Cell::new(available)),
        };
        use gpui_kit::component::table::TableDelegate as _;

        let count = delegate.columns_count(cx);
        let widths: Vec<gpui::Pixels> = (0..count).map(|ix| delegate.column(ix, cx).width).collect();
        let name = widths[1];
        let branch = widths[3]; // shortcuts sits between name and branch
        let others: gpui::Pixels = widths
            .iter()
            .enumerate()
            .filter(|(ix, _)| *ix != 1)
            .map(|(_, w)| *w)
            .sum();

        // branch hugs the widest branch: "release/feature-x" = 17 chars
        assert_eq!(branch, gpui::px(56. + 17. * 7.8));
        assert!(name >= gpui::px(140.), "the name column keeps its floor (got {name})");
        assert!(
            (others + name - available).abs() < gpui::px(1.),
            "columns must fill the available width exactly (others {others} + name {name} vs {available})"
        );

        // a huge branch clamps at the ceiling instead of starving the name
        store.update(cx, |s, _| {
            if let Some(repo) = s.repos.last_mut() {
                repo.git_branch_name =
                    Some("release/an-extremely-long-branch-name-for-clamping".into());
            }
        });
        let branch = delegate.column(3, cx).width;
        assert_eq!(branch, gpui::px(240.), "branch width clamps at 240");
    });
}

/// Narrow windows drop the time/GitHub/Azure columns and finally shrink the
/// changes cell to its badge-only form; at every width the columns still fit
/// the table (no horizontal scrollbar) and the name keeps its floor.
#[gpui::test]
fn columns_fit_narrow_widths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update(|cx| {
        use gpui_kit::component::table::TableDelegate as _;

        for available in [674., 700., 760., 854., 960., 1100., 1234.] {
            let delegate = devtools::ui::repos::ReposDelegate {
                store: store.clone(),
                available_width: std::rc::Rc::new(std::cell::Cell::new(gpui::px(available))),
            };
            let count = delegate.columns_count(cx);
            let widths: Vec<gpui::Pixels> =
                (0..count).map(|ix| delegate.column(ix, cx).width).collect();
            let sum: gpui::Pixels = widths.iter().copied().sum();

            assert!(
                sum <= gpui::px(available) + gpui::px(1.),
                "columns must fit {available}px, got {widths:?} (sum {sum})"
            );
            assert!(
                widths[1] >= gpui::px(140.),
                "the name column keeps its floor at {available}px (got {})",
                widths[1]
            );
            assert!(
                widths[4] >= gpui::px(80.) && widths[4] <= gpui::px(154.),
                "the changes column stays usable at {available}px (got {})",
                widths[4]
            );
            assert_eq!(widths[0], gpui::px(36.), "star column leads at {available}px");
            assert!(
                widths[2] > gpui::px(100.),
                "launcher buttons stay at {available}px (got {})",
                widths[2]
            );
        }
    });
}

/// The toolbar's sort dropdown is gone: the table headers are the only sort
/// controls. A Branch header sort must reach the store as RepoSortMode::Branch
/// and reorder the projection. The header path persists the mode, so the
/// probe runs under a temp HOME.
#[gpui::test]
fn branch_header_sort_reorders_the_projection(cx: &mut TestAppContext) {
    use devtools_core::settings::RepoSortMode;
    use gpui_kit::component::table::{ColumnSort, TableDelegate as _};

    let home = std::env::temp_dir().join(format!("devtools-sort-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".devtools")).unwrap();
    // SAFETY: process-global env; other tests in this binary tolerate a
    // default-settings store, so a racing HOME read is harmless
    unsafe { std::env::set_var("HOME", &home) };

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        // alpha ships as a favorite; favorites float above every sort
        s.repos[0].tags.clear();
        s.repos[0].git_branch_name = Some("zeta".into());
        s.repos[1].git_branch_name = Some("alpha".into());
        // gamma keeps the entry default ("main")
    });

    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    cx.update(|window, cx| {
        let delegate = devtools::ui::repos::ReposDelegate {
            store: store.clone(),
            available_width: std::rc::Rc::new(std::cell::Cell::new(gpui::px(1100.))),
        };
        let branch_ix = (0..delegate.columns_count(cx))
            .find(|&ix| delegate.column(ix, cx).key == "branch")
            .expect("the branch column is visible at 1100px");
        let table = cx.new(|cx| {
            gpui_kit::component::table::TableState::new(delegate, window, cx)
        });
        table.update(cx, |state, cx| {
            state
                .delegate_mut()
                .perform_sort(branch_ix, ColumnSort::Ascending, window, cx);
        });
    });

    let (mode, ascending) = cx.read_entity(&store, |s, _| (s.sort_mode, s.sort_ascending));
    assert_eq!(mode, RepoSortMode::Branch, "the branch header maps to the Branch mode");
    assert!(ascending, "Ascending lands as ascending");
    let order = cx.read_entity(&store, |s, _| {
        s.projected.iter().map(|&i| s.repos[i].name.clone()).collect::<Vec<_>>()
    });
    assert_eq!(
        order,
        vec!["beta", "gamma", "alpha"],
        "branches alpha < main < zeta order the rows"
    );

    let _ = std::fs::remove_dir_all(&home);
}

fn scanned(name: &str, path: &str) -> devtools_core::scan::ScannedRepo {
    devtools_core::scan::ScannedRepo {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
    }
}

fn dialog_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_click(b.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn dialog_draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// The Add dialog: results arrive through the store (the net settle path),
/// Add stays gated until a row is checked — a no-op click must not close the
/// dialog — and Rescan resets to the Scan state through the real field +
/// button interaction.
#[gpui::test]
fn add_dialog_gated_add_and_rescan(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "add-repo-button");
    assert!(cx.debug_bounds("add-repo-overlay").is_some(), "dialog opens");
    assert!(cx.debug_bounds("btn-Scan").is_some(), "the dedicated Scan button renders");
    assert!(
        cx.debug_bounds("add-dialog-close").is_some(),
        "the header close button renders"
    );

    // the net settle path: the worker drops results on the store, the page
    // drains them while drawing
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.deep_scan_result = Some(vec![
            scanned("repo-x", "/probe/scan/repo-x"),
            scanned("repo-y", "/probe/scan/repo-y"),
        ]);
        cx.notify();
    });
    cx.run_until_parked();
    dialog_draw(cx);
    assert!(cx.debug_bounds("btn-accent-Add").is_some(), "Add waits in the footer");
    assert!(cx.debug_bounds("found-repo-0").is_some());

    // uncheck both rows: the disabled Add must be a no-op, not a close
    dialog_click(cx, "found-repo-0");
    dialog_click(cx, "found-repo-1");
    dialog_click(cx, "btn-accent-Add");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "gated Add must not close the dialog"
    );

    // type a fresh path and Scan again: stale rows are cleared, the dialog
    // holds; the results well is back to its empty text
    dialog_click(cx, "add-root-input");
    cx.simulate_input("/probe/scan-root");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-Scan");
    assert!(
        cx.debug_bounds("found-repo-0").is_none(),
        "stale results are cleared on rescan"
    );
    assert!(cx.debug_bounds("add-repo-overlay").is_some());

    // the header X closes the dialog
    dialog_click(cx, "add-dialog-close");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "the X close button dismisses the dialog"
    );
}

/// The Clone dialog: the destination is editable and feeds validation — an
/// empty destination holds the dialog open with the inline error, a valid
/// one lets the commit flip the button to its busy label.
#[gpui::test]
fn clone_dialog_editable_destination_feeds_validation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "clone-button");
    assert!(cx.debug_bounds("clone-overlay").is_some(), "dialog opens");
    assert!(cx.debug_bounds("btn-accent-Clone").is_some());

    // no URL yet: the disabled Clone is a no-op
    dialog_click(cx, "btn-accent-Clone");
    assert!(cx.debug_bounds("clone-overlay").is_some());

    // URL typed, destination cleared: validation must hold the dialog open
    dialog_click(cx, "clone-url-input");
    cx.simulate_input("https://github.com/owner/dt-probe-clone.git");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "clone-dest-input");
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("clone-overlay").is_some(),
        "a missing destination keeps the dialog open"
    );
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_none(),
        "the clone must not have been sent"
    );

    // a valid destination passes validation: the button flips to busy (the
    // net channel has no worker in the probe, so the result never settles)
    dialog_click(cx, "clone-dest-input");
    cx.simulate_input("/tmp");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_some(),
        "validation passed and the commit went out"
    );
    assert!(cx.debug_bounds("clone-overlay").is_some());
}

/// The kit's header sort glyphs and click cycle read the cached column
/// groups, so the delegate must hand back the store's live sort state: the
/// active column carries the direction, everything else stays unsorted.
#[gpui::test]
fn header_columns_mirror_the_active_sort(cx: &mut TestAppContext) {
    use devtools_core::settings::RepoSortMode;
    use gpui_kit::component::table::ColumnSort;

    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        // in-memory only: the probe must not rewrite the real settings file
        s.sort_mode = RepoSortMode::LastActivity;
        s.sort_ascending = false;
    });

    cx.update(|cx| {
        let available = gpui::px(1100.);
        let delegate = devtools::ui::repos::ReposDelegate {
            store: store.clone(),
            available_width: std::rc::Rc::new(std::cell::Cell::new(available)),
        };
        use gpui_kit::component::table::TableDelegate as _;

        // columns: star, name, shortcuts, branch, changes, …, last activity
        let activity_ix = delegate.columns_count(cx) - 1;
        assert!(
            matches!(delegate.column(activity_ix, cx).sort, Some(ColumnSort::Descending)),
            "the active column shows its direction"
        );
        assert!(
            matches!(delegate.column(1, cx).sort, Some(ColumnSort::Default)),
            "inactive columns stay unsorted"
        );
    });

    cx.update_entity(&store, |s: &mut Store, _| {
        s.sort_mode = RepoSortMode::Name;
        s.sort_ascending = true;
    });
    cx.update(|cx| {
        let delegate = devtools::ui::repos::ReposDelegate {
            store: store.clone(),
            available_width: std::rc::Rc::new(std::cell::Cell::new(gpui::px(1100.))),
        };
        use gpui_kit::component::table::TableDelegate as _;

        assert!(
            matches!(delegate.column(1, cx).sort, Some(ColumnSort::Ascending)),
            "switching modes hands the arrow to the new column"
        );
        let activity_ix = delegate.columns_count(cx) - 1;
        assert!(
            matches!(delegate.column(activity_ix, cx).sort, Some(ColumnSort::Default)),
            "the previous column drops back to unsorted"
        );
    });
}
