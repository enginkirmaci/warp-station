//! Headless probe for the titlebar global search: a typed character with no
//! text input focused starts a fresh term there, Ctrl+F pulls the focus back
//! to the field (selecting the term so typing replaces it), and a focused
//! field keeps its own characters.

use devtools::store::{GlobalSearchHit, Store};
use devtools::ui::window::MainWindow;
use devtools_core::code_search::CodeHit;
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
        // pin the flat layout: the developer's live settings must never
        // re-shape the probes' row math (the grouping probes seed their own
        // scratch-settings store)
        store.group_by_root = false;
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

fn code_hit(repo: &str, rel: &str, line: u32, excerpt: &str, start: usize, end: usize) -> CodeHit {
    CodeHit {
        repo: repo.into(),
        rel_path: rel.into(),
        line,
        excerpt: excerpt.into(),
        match_start: start,
        match_end: end,
        abs_path: PathBuf::from(format!("/probe/{repo}/{rel}")),
    }
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
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["alpha", "beta", "gamma"]);
    assert!(
        cx.debug_bounds("search-popup").is_some(),
        "the results popup renders under the titlebar field"
    );

    // the field now owns the focus, so the rest of the word lands in it —
    // and is appended, not treated as another fresh term
    cx.simulate_input("lph");
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
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
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["beta"]);

    // the field owns the focus now, so characters append to the term instead
    // of triggering another type-anywhere pass
    cx.simulate_input("x");
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
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
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
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
/// The pill renders only when the warp-agent door resolves, so the probe
/// backs the store with scratch settings naming an existing executable.
#[gpui::test]
fn titlebar_side_columns_stay_balanced(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let exe = std::env::current_exe().expect("test binary path");
    let json = format!(
        r#"{{"OpenCode": {{"EnableOpenCode": true}}, "WarpAgent": {{"EnableWarpAgent": true, "Executable": "{}"}}}}"#,
        exe.display().to_string().replace('\\', "\\\\")
    );
    let scratch = std::env::temp_dir().join(format!("devtools-probe-titlebar-{}.json", std::process::id()));
    std::fs::write(&scratch, json).expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch.clone())),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
            entry("gamma", "/probe/gamma"),
        ]);
        store
    });
    let _ = std::fs::remove_file(&scratch);
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

/// The whole titlebar must be a drag surface, not just the brand or the
/// spacer strips beside the search field. On Windows the SnapIt/Awake chips
/// widen both side columns until those flex spacers collapse to zero at the
/// default window size, which left the window with no titlebar drag region.
/// One full-width hitbox now covers the bar; the search field and buttons
/// occlude it, so they stay clickable.
#[gpui::test]
fn titlebar_bar_is_a_full_width_drag_surface(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "devtools-probe-titlebar-drag-{}.json",
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
    let handle = cx.open_window(gpui::size(gpui::px(1280.), gpui::px(820.)), |window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    let mut wcx = gpui::VisualTestContext::from_window(handle.into(), cx);
    wcx.run_until_parked();
    draw(&mut wcx);
    let _ = std::fs::remove_file(&scratch);

    let bar = wcx
        .debug_bounds("titlebar-drag")
        .expect("titlebar drag surface");
    assert_eq!(bar.origin, gpui::point(gpui::px(0.), gpui::px(0.)));
    assert_eq!(bar.size.width, gpui::px(1280.));
    assert_eq!(bar.size.height, gpui::px(devtools::theme::TITLEBAR_H));
    // the controls ride above the drag hitbox and stay where they were
    for selector in [
        "titlebar-brand",
        "titlebar-right",
        "tools-toggle",
        "settings-toggle",
    ] {
        let bounds = wcx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} renders"));
        assert!(
            bounds.origin.x >= bar.origin.x
                && bounds.origin.x + bounds.size.width <= bar.origin.x + bar.size.width,
            "{selector} sits inside the drag surface"
        );
    }
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

    // a control above the full-bar drag region still takes its click
    cx.simulate_click(tools.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds("tools-menu").is_some(),
        "the tools button opens its menu through the drag region"
    );
}

/// The popup's code section: ripgrep hits hand-set on the store render as
/// "code" rows after the repo/note rows, under one shared 12-row budget.
#[gpui::test]
fn code_rows_render_below_repo_rows_under_the_shared_cap(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    draw(cx);

    // a 3-character term (the code sweep's lower gate) that matches one repo;
    // the debounced run also dispatches the ripgrep sweep when rg resolves
    cx.simulate_input("alp");
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(repo_hits(&store, cx), ["alpha"]);

    // hand-set the sweep's answer: 14 code hits so the shared cap bites
    // (1 repo row + 11 code rows = 12), and settle the searching hint
    let hits: Vec<CodeHit> = (0..14)
        .map(|i| code_hit("alpha", &format!("src/file{i}.rs"), i + 1, "let needle = 1;", 4, 10))
        .collect();
    store.update(cx, |s, cx| {
        s.code_search_hits = hits;
        s.code_searching = false;
        cx.notify();
    });
    draw(cx);

    assert!(
        cx.debug_bounds("search-hit-0").is_some(),
        "the repo row still renders above the code rows"
    );
    for i in 0..11 {
        // debug_bounds wants a 'static str; the loop is bounded so a leak is fine
        let selector: &'static str = Box::leak(format!("search-code-hit-{i}").into_boxed_str());
        assert!(
            cx.debug_bounds(selector).is_some(),
            "code row {i} renders"
        );
    }
    assert!(
        cx.debug_bounds("search-code-hit-11").is_none(),
        "the popup's 12-row budget caps the code section"
    );
}

/// The generation guard: a settled sweep answers only its own dispatch. A
/// stale generation (a newer keystroke superseded it) drops silently; the
/// current one applies and settles the searching hint.
#[gpui::test]
fn stale_code_search_results_drop_and_current_apply(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let hits = vec![code_hit("alpha", "src/lib.rs", 7, "let needle = 1;", 4, 10)];

    // the store's code generation starts at 0 and a sweep is in flight;
    // anything other than 0 answering back is a superseded run
    store.update(cx, |s, cx| {
        s.code_searching = true;
        cx.notify();
    });
    store.update(cx, |s, cx| {
        s.on_net_event(devtools::net::UiEvent::CodeSearchSettled { generation: 9, hits: hits.clone() }, cx);
    });
    let (applied, searching) = cx.read_entity(&store, |s, _| (s.code_search_hits.is_empty(), s.code_searching));
    assert!(applied, "a stale sweep must not land");
    assert!(searching, "the in-flight hint survives a stale settle");

    store.update(cx, |s, cx| {
        s.on_net_event(devtools::net::UiEvent::CodeSearchSettled { generation: 0, hits: hits.clone() }, cx);
    });
    let (applied, searching) = cx.read_entity(&store, |s, _| (s.code_search_hits.len(), s.code_searching));
    assert_eq!(applied, 1, "the current generation's sweep lands");
    assert!(!searching, "applying the settle clears the searching hint");
}

/// The launch decision for an activated code hit is pure: a resolvable
/// executable plans the VS Code `--goto` launch (profile included), an
/// unresolvable one plans the system-default fallback. No spawn here.
#[test]
fn code_open_plan_picks_vscode_or_fallback() {
    use devtools_core::terminal::{plan_code_open, CodeOpenPlan};

    let missing = plan_code_open("/probe/alpha/src/lib.rs", 42, "devtools-definitely-missing-binary", "");
    assert_eq!(
        missing,
        CodeOpenPlan::SystemDefault { file: "/probe/alpha/src/lib.rs".into() },
        "an unresolvable executable falls back to the system default"
    );

    // an existing absolute path (this test binary) plans the VS Code launch
    let exe = std::env::current_exe().expect("test binary path");
    let plan = plan_code_open("/probe/alpha/src/lib.rs", 42, &exe.to_string_lossy(), "rust");
    assert_eq!(
        plan,
        CodeOpenPlan::VsCode {
            executable: exe,
            profile: "rust".into(),
            file: "/probe/alpha/src/lib.rs".into(),
            line: 42,
        },
        "a resolvable executable plans the file:line launch with the profile"
    );
}
