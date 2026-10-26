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

fn px_col() -> gpui::Pixels {
    gpui::px(120.)
}

fn px_row() -> gpui::Pixels {
    // titlebar 38 + toolbar ~44 (no totals row, header collapsed) + row center
    gpui::px(100.)
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
        let branch = widths[3];
        let others: gpui::Pixels = widths
            .iter()
            .enumerate()
            .filter(|(ix, _)| *ix != 1)
            .map(|(_, w)| *w)
            .sum();

        // branch hugs the widest branch: "release/feature-x" = 17 chars
        assert_eq!(branch, gpui::px(52. + 17. * 7.2));
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

/// Press + release at a point through the real event pipeline, then settle
/// and redraw so `debug_bounds` reflects the new frame.
fn drive_click(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

#[gpui::test]
fn sort_menu_opens_above_the_table_and_dismisses(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, mut cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // open through the chip's real click path
    let chip = cx
        .debug_bounds("dt-sort-chip")
        .expect("sort chip rendered in the toolbar");
    drive_click(&mut cx, chip.center());

    let menu = cx
        .debug_bounds("dt-sort-menu")
        .expect("sort menu visible after clicking the chip");
    let scrim = cx
        .debug_bounds("dt-sort-scrim")
        .expect("full-page click catcher visible");
    // the popup sits below the chip and fully inside the window
    assert!(menu.origin.y >= chip.bottom(), "menu must drop below the chip");
    assert!(
        menu.origin.x >= scrim.origin.x
            && menu.bottom() <= scrim.bottom()
            && menu.right() <= scrim.right(),
        "menu must stay inside the window ({menu:?} vs scrim {scrim:?})"
    );

    // a click over the table area must dismiss the menu WITHOUT reaching the
    // row beneath: the pre-fix in-toolbar catcher buried the popup under the
    // table, and this same click selected a row instead
    let over_table = gpui::point(scrim.origin.x + px_col(), px_row());
    drive_click(&mut cx, over_table);
    assert!(
        cx.debug_bounds("dt-sort-menu").is_none(),
        "clicking outside must close the menu"
    );
    let selected = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    assert!(
        selected.is_none(),
        "the dismiss click must not select a table row (got {selected:?})"
    );

    // the chip still toggles afterwards
    let chip = cx
        .debug_bounds("dt-sort-chip")
        .expect("sort chip still present");
    drive_click(&mut cx, chip.center());
    assert!(
        cx.debug_bounds("dt-sort-menu").is_some(),
        "menu must reopen from the chip"
    );
}
