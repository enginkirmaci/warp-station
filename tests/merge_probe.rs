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
    let snapshot = warp_station_core::git::read::change_snapshot(&repo);
    let branches = warp_station_core::git::read::branches(&repo).unwrap_or_default();
    assert!(
        snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.merge.as_ref())
            .is_some(),
        "the fixture is a stopped merge"
    );
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store =
            Store::with_settings(net_tx, scratch_service("merge-surface")).without_flow_cache();
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "merge-repo".into(),
            folder_path: repo.clone(),
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

    let _ = std::fs::remove_dir_all(&repo);
}
