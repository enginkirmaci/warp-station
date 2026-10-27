//! Headless probe for the titlebar global search: a typed character with no
//! text input focused starts a fresh term there, Ctrl+F pulls the focus back
//! to the field (selecting the term so typing replaces it), and a focused
//! field keeps its own characters.

use devtools::store::{GlobalSearchHit, Store};
use devtools::ui::window::MainWindow;
use devtools_core::repo_store::RepoEntry;
use gpui::{prelude::*, TestAppContext};
use std::path::PathBuf;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
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
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
            entry("gamma", "/probe/gamma"),
        ]);
        store
    })
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn repo_hits(store: &gpui::Entity<Store>, cx: &gpui::TestAppContext) -> Vec<String> {
    cx.read_entity(store, |s, _| {
        s.global_search
            .iter()
            .filter_map(|hit| match hit {
                GlobalSearchHit::Repo { name, .. } => Some(name.clone()),
                GlobalSearchHit::Note { .. } => None,
            })
            .collect()
    })
}

#[gpui::test]
fn typing_anywhere_runs_the_global_search(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    draw(cx);

    // the window root owns the focus at startup: the first typed character
    // must start a fresh term in the global search
    cx.simulate_input("a");
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["alpha", "beta", "gamma"]);
    assert!(
        cx.debug_bounds("search-popup").is_some(),
        "the results popup renders under the titlebar field"
    );

    // the field now owns the focus, so the rest of the word lands in it —
    // and is appended, not treated as another fresh term
    cx.simulate_input("lph");
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["alpha"]);

    // the term lives in the popup search only — the table keeps every row
    assert_eq!(cx.read_entity(&store, |s, _| s.projected.len()), 3);
}

#[gpui::test]
fn ctrl_f_selects_the_term_so_typing_replaces_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    draw(cx);

    // type-anywhere builds a term and hands the focus to the global field
    cx.simulate_input("bet");
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["beta"]);

    // the field owns the focus now, so characters append to the term instead
    // of triggering another type-anywhere pass
    cx.simulate_input("x");
    draw(cx);
    assert_eq!(
        repo_hits(&store, cx),
        Vec::<String>::new(),
        "a focused field keeps its own characters"
    );

    // Ctrl+F selects the term, so typing replaces it instead of appending
    // (appending would leave "betxa" matching nothing)
    cx.simulate_keystrokes("ctrl-f");
    cx.simulate_input("a");
    draw(cx);
    assert_eq!(
        repo_hits(&store, cx),
        ["alpha", "beta", "gamma"],
        "typing after Ctrl+F must replace the selected term"
    );
}

/// The brand column (app name + notes + summary) and the right action group
/// share one fixed width — that is what keeps the search field centered, so
/// the summary pill moving into the titlebar must not unbalance the sides.
#[gpui::test]
fn titlebar_side_columns_stay_balanced(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    draw(cx);

    let brand = cx.debug_bounds("titlebar-brand").expect("brand column");
    let right = cx.debug_bounds("titlebar-right").expect("right group");
    assert!(
        (brand.size.width - right.size.width).abs() < gpui::px(2.),
        "side columns must stay equal for a centered search \
         (brand {brand:?} vs right {right:?})"
    );
    assert!(
        cx.debug_bounds("summary-button").is_some(),
        "the summary pill renders in the titlebar"
    );
    let summary = cx.debug_bounds("summary-button").expect("summary pill");
    assert!(
        summary.origin.x + summary.size.width <= brand.origin.x + brand.size.width + gpui::px(1.),
        "the summary pill must fit the brand column (pill ends at {}, column at {})",
        summary.origin.x + summary.size.width,
        brand.origin.x + brand.size.width
    );
}

/// The titlebar's right section reads left to right: status chips (NuGet,
/// SnapIt on Windows), a separator, the Tools+Settings pill, another
/// separator, then the window controls.
#[gpui::test]
fn titlebar_right_orders_chips_tools_then_window_buttons(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    // scratch settings: the probe must not depend on the real file's
    // EnableNuget flag ({} = all defaults, NuGet on)
    let scratch = std::env::temp_dir().join(format!(
        "devtools-probe-titlebar-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch.clone())),
        )
    });
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    let _ = std::fs::remove_file(&scratch);

    let nuget = cx.debug_bounds("nuget-status").expect("nuget chip renders");
    let tools = cx.debug_bounds("tools-toggle").expect("tools button");
    let settings = cx.debug_bounds("settings-toggle").expect("settings button");
    let minimize = cx.debug_bounds("minimize-window").expect("minimize button");

    assert!(
        nuget.right() <= tools.origin.x,
        "the NuGet chip sits left of the tool pill ({:?} vs {:?})",
        nuget,
        tools
    );
    assert!(
        tools.right() <= settings.origin.x,
        "tools and settings share the pill, tools first"
    );
    assert!(
        settings.right() <= minimize.origin.x,
        "the window controls come after a separator ({:?} vs {:?})",
        settings,
        minimize
    );
}
