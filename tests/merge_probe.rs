//! Headless probe for the merge surface: a real stopped merge drives the
//! Changes band, the branch row's context menu and the Local/Incoming editor.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point};
use warp_station::net::{GitAction, NetCommand, UiEvent};
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::window::MainWindow;

fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-merge-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .envs([
            ("GIT_AUTHOR_NAME", "t"),
            ("GIT_AUTHOR_EMAIL", "t@example.com"),
            ("GIT_COMMITTER_NAME", "t"),
            ("GIT_COMMITTER_EMAIL", "t@example.com"),
        ])
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// `main` and `feat` edit the same line, so `git merge feat` stops.
fn stopped_merge(name: &str) -> PathBuf {
    stopped_merge_style(name, false)
}

/// The same fixture; `diff3` makes git write a base section into the markers.
fn stopped_merge_style(name: &str, diff3: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-probe-merge-repo-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    git(&dir, &["init", "-b", "main"]);
    std::fs::write(dir.join("shared.txt"), "base\n").expect("base");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "base"]);
    git(&dir, &["checkout", "-b", "feat"]);
    std::fs::write(dir.join("shared.txt"), "feat\n").expect("feat");
    git(&dir, &["commit", "-am", "feat edit"]);
    git(&dir, &["checkout", "main"]);
    std::fs::write(dir.join("shared.txt"), "main\n").expect("main");
    git(&dir, &["commit", "-am", "main edit"]);
    if diff3 {
        git(&dir, &["config", "merge.conflictStyle", "diff3"]);
    }
    let status = std::process::Command::new("git")
        .args(["merge", "feat"])
        .current_dir(&dir)
        .status()
        .expect("git runs");
    assert!(!status.success(), "the fixture merge must stop");
    dir
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be on screen"));
    cx.simulate_click(
        point(bounds.center().x, bounds.center().y),
        Modifiers::default(),
    );
}

/// Opens the editor from the pane's button and hands it the file the net
/// thread would load.
fn open_editor(
    cx: &mut VisualTestContext,
    repo: &Path,
    store: &gpui::Entity<Store>,
    net_rx: &async_channel::Receiver<NetCommand>,
) {
    click(cx, "open-merge-editor");
    cx.run_until_parked();
    drain(net_rx);
    let file = warp_station_core::git::conflict::conflict_file(repo, "shared.txt")
        .expect("the conflict file");
    let folder = repo.to_string_lossy().to_string();
    cx.update_entity(store, |store, cx| {
        store.on_net_event(
            UiEvent::MergeConflictLoaded {
                folder,
                path: "shared.txt".into(),
                file: Some(file),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// A store primed with the fixture's snapshot, branches and Changes tab.
fn probe_store(
    cx: &mut TestAppContext,
    repo: &Path,
    name: &str,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    let folder = repo.to_string_lossy().to_string();
    let snapshot = warp_station_core::git::read::change_snapshot(repo);
    let branches = warp_station_core::git::read::branches(repo).unwrap_or_default();
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, scratch_service(name)).without_flow_cache();
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "merge-repo".into(),
            folder_path: repo.to_path_buf(),
            solution_path: None,
            tags: vec![],
            git_branch_name: Some("main".into()),
            git_modified_count: 1,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
            azure_pull_count: 0,
            github_web_url: None,
            azure_work_item_count: 0,
        }]);
        store.select_repo(&folder, cx);
        store.set_changes(snapshot);
        store.branches = branches;
        store.changes_loaded_for = Some(folder.clone());
        store.open_detail_tab(WorkspaceTab::Changes, cx);
        store
    });
    (store, net_rx)
}

fn right_click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be on screen"));
    let at = point(bounds.center().x, bounds.center().y);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
}

fn drain(rx: &async_channel::Receiver<NetCommand>) -> Vec<NetCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = rx.try_recv() {
        commands.push(command);
    }
    commands
}

#[gpui_kit::gpui::test]
fn a_stopped_merge_draws_the_band_menu_and_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = stopped_merge("surface");
    let folder = repo.to_string_lossy().to_string();
    let (store, net_rx) = probe_store(cx, &repo, "merge-surface");
    assert!(
        cx.read_entity(&store, |store, _| store.merge_state().is_some()),
        "the fixture is a stopped merge"
    );
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("merge-band").is_some(),
        "the band is the first row of the Changes body"
    );
    assert!(
        cx.debug_bounds("open-merge-editor").is_some(),
        "the conflicted file's pane offers the editor"
    );

    // ---- the branch row's context menu ----
    drain(&net_rx);
    click(cx, "branch-button");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    let trigger = cx
        .debug_bounds("branch-button")
        .expect("the branch trigger");
    let panel = cx.debug_bounds("branch-menu").expect("the branch panel");
    assert!(
        f32::from(panel.origin.y) >= f32::from(trigger.origin.y + trigger.size.height),
        "the branch panel opens below its trigger (trigger bottom {}, panel top {})",
        f32::from(trigger.origin.y + trigger.size.height),
        f32::from(panel.origin.y)
    );
    let row = cx
        .debug_bounds("branch-row-feat")
        .expect("the feat row is in the open list");
    let at = point(row.center().x, row.center().y);
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu-catcher").is_some(),
        "a right-click opens the row menu"
    );
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown stays open under the row menu"
    );
    assert!(
        cx.debug_bounds("branch-menu-merge").is_some(),
        "its first verb is Merge"
    );
    // a stopped merge is already in progress: Merge is present but disabled
    assert!(
        cx.debug_bounds("branch-menu-merge").is_some(),
        "the Merge verb stays visible while a merge is stopped"
    );
    click(cx, "branch-menu-checkout");
    cx.run_until_parked();
    let commands = drain(&net_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            NetCommand::Git {
                action: GitAction::Checkout(branch),
                ..
            } if branch == "feat"
        )),
        "the menu's Checkout runs on the row's branch"
    );
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "the action closes the dropdown with the menu"
    );

    // ---- the editor ----
    click(cx, "open-merge-editor");
    cx.run_until_parked();
    let requested = drain(&net_rx);
    assert!(
        requested.iter().any(|command| matches!(
            command,
            NetCommand::LoadMergeConflict { path, .. } if path == "shared.txt"
        )),
        "opening the editor reads the conflict"
    );
    let file = warp_station_core::git::conflict::conflict_file(&repo, "shared.txt")
        .expect("the fixture's conflict file");
    assert_eq!(file.blocks.len(), 1, "one marker block");
    cx.update_entity(&store, |store, cx| {
        store.on_net_event(
            UiEvent::MergeConflictLoaded {
                folder: folder.clone(),
                path: "shared.txt".into(),
                file: Some(file),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("merge-editor").is_some(),
        "the editor takes the body"
    );
    assert!(
        cx.debug_bounds("merge-block-0").is_some(),
        "the conflict strip numbers the block"
    );
    assert!(
        cx.debug_bounds("merge-local-take").is_some()
            && cx.debug_bounds("merge-incoming-take").is_some(),
        "both sides carry a take checkbox"
    );
    assert!(
        cx.debug_bounds("merge-accept").is_some(),
        "Accept Merge is in the head"
    );
    click(cx, "merge-local-take");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "merge-incoming-take");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-result-scroll").is_some(),
        "the result pane renders the assembly"
    );
    assert!(
        cx.debug_bounds("merge-local").is_some(),
        "the whole-file Local pane renders"
    );
    assert!(
        cx.debug_bounds("merge-layout").is_some(),
        "a wide pane offers the column layout"
    );
    assert!(
        cx.debug_bounds("merge-base-toggle").is_none(),
        "a merge-style conflict has no base sections, so no Base toggle"
    );
    // the column layout keeps every pane and the result on one row
    click(cx, "merge-layout");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-local").is_some()
            && cx.debug_bounds("merge-result-scroll").is_some(),
        "the column layout still shows the files and the result"
    );

    // ---- the conflicted row's bulk menu ----
    click(cx, "merge-back");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    drain(&net_rx);
    right_click(cx, "u-shared.txt");
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("file-menu-catcher").is_some(),
        "a right-click on the conflicted row opens its menu"
    );
    assert!(
        cx.debug_bounds("file-menu-local").is_some()
            && cx.debug_bounds("file-menu-incoming").is_some(),
        "both whole-file sides are offered"
    );
    click(cx, "file-menu-incoming");
    cx.run_until_parked();
    let bulk = drain(&net_rx);
    assert!(
        bulk.iter().any(|command| matches!(
            command,
            NetCommand::Git {
                action: GitAction::ResolveConflictBulk { path, side },
                ..
            } if path == "shared.txt"
                && *side == warp_station_core::git::conflict::ConflictSide::Incoming
        )),
        "the menu's whole-file take reaches the net thread"
    );

    let _ = std::fs::remove_dir_all(&repo);
}

/// A diff3 conflict carries a base section, so the head offers the Base pane
/// and the pane shows the ancestor's file.
#[gpui_kit::gpui::test]
fn a_diff3_conflict_offers_the_base_pane(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = stopped_merge_style("diff3", true);
    let folder = repo.to_string_lossy().to_string();
    let (store, net_rx) = probe_store(cx, &repo, "merge-diff3");
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "open-merge-editor");
    cx.run_until_parked();
    drain(&net_rx);
    let file = warp_station_core::git::conflict::conflict_file(&repo, "shared.txt")
        .expect("the diff3 conflict file");
    assert!(
        file.blocks.iter().all(|block| block.base.is_some()),
        "diff3 writes a base section"
    );
    cx.update_entity(&store, |store, cx| {
        store.on_net_event(
            UiEvent::MergeConflictLoaded {
                folder: folder.clone(),
                path: "shared.txt".into(),
                file: Some(file),
            },
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("merge-base-toggle").is_some(),
        "a diff3 conflict offers the Base toggle"
    );
    assert!(
        cx.debug_bounds("merge-base").is_none(),
        "the Base pane starts hidden"
    );
    click(cx, "merge-base-toggle");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-base").is_some(),
        "the toggle shows the Base pane"
    );

    let _ = std::fs::remove_dir_all(&repo);
}

/// Take both writes Local then Incoming into the Result, and Accept sends
/// that text to git.
#[gpui_kit::gpui::test]
fn taking_both_sides_sends_both_to_the_result(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = stopped_merge("both");
    let (store, net_rx) = probe_store(cx, &repo, "merge-both");
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    open_editor(cx, &repo, &store, &net_rx);

    click(cx, "merge-both-local-first");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    click(cx, "merge-accept");
    cx.run_until_parked();
    let commands = drain(&net_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            NetCommand::Git {
                action: GitAction::ResolveConflict { path, text: Some(text) },
                ..
            } if path == "shared.txt" && text == "main\nfeat\n"
        )),
        "Accept sends the both-side result"
    );

    let _ = std::fs::remove_dir_all(&repo);
}

/// Two presses inside the double-click window open the conflicted row's
/// editor.
#[gpui_kit::gpui::test]
fn double_clicking_a_conflicted_row_opens_the_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = stopped_merge("double-click");
    let (store, _net_rx) = probe_store(cx, &repo, "merge-double");
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "u-shared.txt");
    cx.run_until_parked();
    click(cx, "u-shared.txt");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-editor").is_some(),
        "the second press opens the merge editor"
    );

    let _ = std::fs::remove_dir_all(&repo);
}
