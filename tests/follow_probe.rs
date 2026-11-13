//! The Pipelines follow loop in a headless window: an open run is re-read on
//! its source's cadence while it can still change, the failed log is asked for
//! once it finishes, and the list behind it gets one last call. No network:
//! the store is seeded with the snapshot and the answers arrive as UiEvents,
//! exactly as the net thread would hand them over.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use warp_station::net::{NetCommand, UiEvent};
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::window::MainWindow;
use warp_station_core::activity::{ActivityRun, RunDetail, RunJob, RunStage, RunState};
use warp_station_core::azure::{AzureSnapshot, PipelineRun};
use warp_station_core::repo_store::RepoEntry;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
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

/// The Azure column on with a token and GitHub off: the follow loop's commands
/// can only be the Azure ones, so the queue reads plainly.
fn scratch_store(
    cx: &mut TestAppContext,
    name: &str,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    // one settings file per probe: the two tests share a process and would
    // otherwise write each other's switches
    let scratch = std::env::temp_dir().join(format!(
        "warp-station-probe-follow-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(warp_station_core::settings::SettingsService::at(scratch)),
        );
        store.group_by_root = false;
        store.update_settings(|settings| {
            settings.repos.enable_github = false;
            settings.repos.enable_azure_devops = true;
            settings.repos.azure_devops_pat = "pat".into();
        });
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    (store, net_rx)
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn open_window<'a>(
    store: &gpui::Entity<Store>,
    cx: &'a mut TestAppContext,
) -> &'a mut gpui::VisualTestContext {
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    cx
}

/// One build of the probe's repository.
fn build(id: u64, status: &str, result: Option<&str>) -> PipelineRun {
    PipelineRun {
        id,
        build_number: format!("20261111.{id}"),
        definition_name: Some("kits".into()),
        branch: Some("refs/heads/main".into()),
        result: result.map(str::to_string),
        status: Some(status.into()),
        url: Some(format!(
            "https://dev.azure.com/o/p/_build/results?buildId={id}"
        )),
        source_version: Some("360efe9b".into()),
        reason: Some("individualCI".into()),
        start: Some("2026-11-11T15:30:00Z".into()),
        finish: None,
        at: Some("2026-11-11T15:29:40Z".into()),
    }
}

/// The detail Azure answers with: the stages and jobs of `kits #48`.
fn detail(state: RunState, log: &str) -> RunDetail {
    RunDetail {
        sha: Some("360efe9b".into()),
        trigger: Some("individualCI".into()),
        start: Some("2026-11-11T15:30:00Z".into()),
        state: Some(state),
        stages: vec![RunStage {
            name: "fmt".into(),
            jobs: vec![RunJob {
                name: "cargo fmt --check".into(),
                state: Some("inProgress".into()),
                result: None,
                start: None,
                finish: None,
                steps: Vec::new(),
            }],
        }],
        log: log.to_string(),
        ..Default::default()
    }
}

fn seed(store: &gpui::Entity<Store>, cx: &mut TestAppContext) {
    cx.update_entity(store, |s: &mut Store, cx| {
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(AzureSnapshot {
                available: true,
                url: Some("https://dev.azure.com/o/p/_git/alpha".into()),
                pipelines: vec![build(48, "inProgress", None)],
                ..Default::default()
            }),
        );
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(WorkspaceTab::Pipelines, cx);
        cx.notify();
    });
}

/// Every run-detail ask since the last drain, with its log flag.
fn detail_asks(net_rx: &async_channel::Receiver<NetCommand>) -> Vec<(u64, bool)> {
    std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter_map(|command| match command {
            NetCommand::AzureRunDetail {
                build_id, with_log, ..
            } => Some((build_id, with_log)),
            _ => None,
        })
        .collect()
}

/// Whether the list's lightweight call was asked for since the last drain.
fn list_asks(net_rx: &async_channel::Receiver<NetCommand>) -> usize {
    std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter(|command| matches!(command, NetCommand::AzureRuns { .. }))
        .count()
}

/// A running build is followed: the open's own read, one poll a cadence, the
/// failed log once it finishes, and one last list call. Nothing follows a run
/// that is already over.
#[gpui::test]
fn a_running_build_is_followed_to_its_end(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "follow");
    seed(&store, cx);
    let cx = open_window(&store, cx);
    // the wide pane opens its first row by itself; a narrow one leaves the
    // pick to the reader, so the probe picks it
    if store.read_with(cx, |s, _| s.open_run.is_none()) {
        let run = ActivityRun::of_azure(&build(48, "inProgress", None));
        cx.update_entity(&store, |s: &mut Store, cx| s.open_run(run, cx));
        draw(cx);
    }

    // the row is running: it opens without its log
    assert_eq!(
        detail_asks(&net_rx),
        [(48, false)],
        "one ask, no log while it is going"
    );
    let open = store
        .read_with(cx, |s, _| s.open_run.clone())
        .expect("a run open");
    assert_eq!(open.run.state, RunState::Running);
    assert!(
        store.read_with(cx, |s, _| s
            .following_run(warp_station_core::activity::ActivitySource::Azure, 48)),
        "the run is being followed"
    );
    assert!(
        cx.debug_bounds("run-following").is_some(),
        "the strip says the pane is following it"
    );

    // the first answer lands: the run is still going, so the loop waits its
    // cadence and then asks again
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Running, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(detail_asks(&net_rx), [], "no poll before the cadence");

    let now = Instant::now();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.check_run_follow(now + Duration::from_secs(22), cx)
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [(48, false)],
        "the poll after the Azure cadence, still without the log"
    );

    // the window is down to the reserve: the next cadence is parked until the
    // platform's own reset, and only then asks again
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs() as i64
        + 120;
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Running, "")),
                budget: warp_station_core::rate::RateBudget {
                    remaining: Some(1),
                    reset: Some(reset),
                    limited: false,
                },
            },
            cx,
        );
    });
    draw(cx);
    let parked = Instant::now();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.check_run_follow(parked + Duration::from_secs(31), cx)
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [],
        "a spent window is not polled on the plain cadence"
    );
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.check_run_follow(parked + Duration::from_secs(121), cx)
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [(48, false)],
        "the poll comes back once the platform's window refills"
    );

    // the run finishes: the answer carries no log, so one more call asks for
    // the failed steps before the loop lets go
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Failed, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [(48, true)],
        "the finished run's log is the last detail call"
    );
    assert!(
        store.read_with(cx, |s, _| s
            .following_run(warp_station_core::activity::ActivitySource::Azure, 48)),
        "the follow holds until the log answer lands"
    );

    // the log lands: the loop stops and asks the list for its last word
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Failed, "##[error]cargo fmt --check\n")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        !store.read_with(cx, |s, _| s
            .following_run(warp_station_core::activity::ActivitySource::Azure, 48)),
        "a finished run is not followed"
    );
    assert_eq!(list_asks(&net_rx), 1, "one list call closes the run out");
    assert!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.state))
            == Some(RunState::Failed),
        "the head takes the run's own state"
    );
    let log = store.read_with(cx, |s, _| {
        s.open_run
            .as_ref()
            .and_then(|open| open.detail.as_ref())
            .map(|detail| detail.log.clone())
    });
    assert_eq!(
        log.as_deref(),
        Some("##[error]cargo fmt --check\n"),
        "the failed log stayed on screen"
    );

    // the list answer flips the row the pass no longer watches
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunsLoaded {
                folder: "/probe/alpha".into(),
                runs: Some(vec![build(48, "completed", Some("failed"))]),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    let status = store.read_with(cx, |s, _| {
        s.azure_by_folder
            .get("/probe/alpha")
            .and_then(|snapshot| snapshot.pipelines.first())
            .and_then(|run| run.result.clone())
    });
    assert_eq!(status.as_deref(), Some("failed"));

    // and nothing keeps asking once the run is over
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.check_run_follow(now + Duration::from_secs(120), cx)
    });
    draw(cx);
    assert_eq!(detail_asks(&net_rx), [], "the loop is done with this run");
    assert_eq!(list_asks(&net_rx), 0, "and with its list");
}

/// The same loop on the Actions side: a running workflow is re-read on the
/// faster GitHub cadence, and its failed log is the last call before the list
/// flips the row.
#[gpui::test]
fn a_running_workflow_is_followed_on_the_github_cadence(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "workflow");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| {
            settings.repos.enable_github = true;
            // the probe's own binary stands in for gh: the store only has to
            // resolve one, the command is captured below and never run
            settings.repos.github_executable = std::env::current_exe()
                .expect("the test binary")
                .to_string_lossy()
                .to_string();
            settings.repos.enable_azure_devops = false;
        });
        s.github_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(warp_station_core::github::GitHubSnapshot {
                available: true,
                url: Some("https://github.com/o/alpha".into()),
                runs: vec![warp_station_core::github::GitHubRun {
                    id: 31,
                    number: 48,
                    name: "kits".into(),
                    title: "Land the follow loop".into(),
                    branch: Some("main".into()),
                    status: Some("in_progress".into()),
                    conclusion: None,
                    url: Some("https://github.com/o/alpha/actions/runs/31".into()),
                    created_at: Some("2026-11-11T15:30:00Z".into()),
                }],
                ..Default::default()
            }),
        );
        s.select_repo("/probe/alpha", cx);
        s.open_detail_tab(WorkspaceTab::Pipelines, cx);
        cx.notify();
    });
    let cx = open_window(&store, cx);
    if store.read_with(cx, |s, _| s.open_run.is_none()) {
        let run = store
            .read_with(cx, |s, _| s.github_by_folder.get("/probe/alpha").cloned())
            .and_then(|snapshot| snapshot.runs.first().map(ActivityRun::of_github));
        let run = run.expect("the seeded run");
        cx.update_entity(&store, |s: &mut Store, cx| s.open_run(run, cx));
        draw(cx);
    }

    let asks = |net_rx: &async_channel::Receiver<NetCommand>| -> Vec<(u64, bool)> {
        std::iter::from_fn(|| net_rx.try_recv().ok())
            .filter_map(|command| match command {
                NetCommand::GitHubRunDetail {
                    run_id, with_log, ..
                } => Some((run_id, with_log)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(asks(&net_rx), [(31, false)], "one ask, no log");

    let now = Instant::now();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubRunDetailLoaded {
                folder: "/probe/alpha".into(),
                run_id: 31,
                budget: Default::default(),
                detail: Some(Box::new(RunDetail {
                    state: Some(RunState::Running),
                    ..Default::default()
                })),
            },
            cx,
        );
        s.check_run_follow(now + Duration::from_secs(21), cx);
    });
    draw(cx);
    assert_eq!(
        asks(&net_rx),
        [(31, false)],
        "the GitHub cadence is shorter than Azure's, and still log-free"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubRunDetailLoaded {
                folder: "/probe/alpha".into(),
                run_id: 31,
                budget: Default::default(),
                detail: Some(Box::new(RunDetail {
                    state: Some(RunState::Failed),
                    ..Default::default()
                })),
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(
        asks(&net_rx),
        [(31, true)],
        "the finished run's failed log is the last detail call"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitHubRunDetailLoaded {
                folder: "/probe/alpha".into(),
                run_id: 31,
                budget: Default::default(),
                detail: Some(Box::new(RunDetail {
                    state: Some(RunState::Failed),
                    log: "##[error]cargo clippy\n".into(),
                    ..Default::default()
                })),
            },
            cx,
        );
    });
    draw(cx);
    let lists = std::iter::from_fn(|| net_rx.try_recv().ok())
        .filter(|command| matches!(command, NetCommand::GitHubRuns { .. }))
        .count();
    assert_eq!(lists, 1, "one list call closes the run out");
    assert!(
        !store.read_with(cx, |s, _| s
            .following_run(warp_station_core::activity::ActivitySource::GitHub, 31)),
        "a finished workflow is not followed"
    );
}

/// A watched run raises one end toast, on the first answer that says it
/// finished; the log call that closes the follow does not repeat it.
#[gpui::test]
fn a_watched_run_raises_one_end_toast(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "toast");
    seed(&store, cx);
    let cx = open_window(&store, cx);
    if store.read_with(cx, |s, _| s.open_run.is_none()) {
        let run = ActivityRun::of_azure(&build(48, "inProgress", None));
        cx.update_entity(&store, |s: &mut Store, cx| s.open_run(run, cx));
        draw(cx);
    }
    let _ = detail_asks(&net_rx);
    // the opening answer says the run is going
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Running, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        store.read_with(cx, |s, _| s.notices.list().is_empty()),
        "a run still going says nothing"
    );

    // the first finished answer is the event; the pane then asks for the log
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Failed, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    let notices = store.read_with(cx, |s, _| {
        s.notices
            .list()
            .iter()
            .map(|notice| notice.text.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        notices
            .iter()
            .filter(|text| text.contains("failed"))
            .count(),
        1,
        "one toast for the finish: {notices:?}"
    );
    assert_eq!(detail_asks(&net_rx), [(48, true)], "the log is still asked");

    // the closing log answer must not raise a second toast
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Failed, "##[error]cargo fmt --check\n")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    let repeats = store.read_with(cx, |s, _| {
        s.notices
            .list()
            .iter()
            .filter(|notice| notice.text.contains("failed"))
            .count()
    });
    assert_eq!(repeats, 1, "the closing answer does not repeat the toast");
}

/// The merge watch's announce flag covers a run that finished before its
/// detail landed; the button's own press does not (the reader is looking at
/// the pane already).
#[gpui::test]
fn a_merge_watch_announces_a_run_that_ended_before_the_detail(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "announce");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(AzureSnapshot {
                available: true,
                pipelines: vec![build(48, "completed", Some("succeeded"))],
                ..Default::default()
            }),
        );
        s.select_repo("/probe/alpha", cx);
    });
    let cx = open_window(&store, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.watch_pipelines_for("main".into(), None, true, cx)
    });
    draw(cx);
    assert_eq!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.id)),
        Some(48),
        "the branch's run is opened"
    );
    let _ = detail_asks(&net_rx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Succeeded, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        store.read_with(cx, |s, _| s
            .notices
            .list()
            .iter()
            .any(|notice| notice.text.contains("passed"))),
        "a run that ended before the detail still announces its result"
    );
}

/// The background switch continues a run the user is already watching when
/// Pipelines is left; with it off, a hidden tab spends no requests.
#[gpui::test]
fn a_watched_run_keeps_polling_after_leaving_pipelines(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "background");
    seed(&store, cx);
    let cx = open_window(&store, cx);
    if store.read_with(cx, |s, _| s.open_run.is_none()) {
        let run = ActivityRun::of_azure(&build(48, "inProgress", None));
        cx.update_entity(&store, |s: &mut Store, cx| s.open_run(run, cx));
        draw(cx);
    }
    let _ = detail_asks(&net_rx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunDetailLoaded {
                folder: "/probe/alpha".into(),
                build_id: 48,
                detail: Some(detail(RunState::Running, "")),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    let later = Instant::now() + Duration::from_secs(22);
    cx.update_entity(&store, |s: &mut Store, cx| {
        // the shipped default is on: this half proves the off switch costs
        // nothing, then the on switch is restored for the second half
        s.update_settings(|settings| settings.workflow.watch_in_background = false);
        s.open_detail_tab(WorkspaceTab::Changes, cx);
        s.check_run_follow(later, cx);
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [],
        "a hidden tab with the switch off spends nothing"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| settings.workflow.watch_in_background = true);
        s.check_run_follow(later, cx);
    });
    draw(cx);
    assert_eq!(
        detail_asks(&net_rx),
        [(48, false)],
        "the watched run keeps its detail cadence off screen"
    );
}

/// A merged pull request's branch focus waits for the merge-triggered run in
/// the lightweight list, opens it, and clears its wait; the chip's clear puts
/// the whole list back.
#[gpui::test]
fn a_merged_branch_focus_waits_for_its_run(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "focus");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(AzureSnapshot {
                available: true,
                ..Default::default()
            }),
        );
        s.select_repo("/probe/alpha", cx);
    });
    let cx = open_window(&store, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.watch_pipelines_for("refs/heads/main".into(), None, false, cx)
    });
    draw(cx);
    assert_eq!(
        store.read_with(cx, |s, _| s.active_detail_tab()),
        WorkspaceTab::Pipelines,
        "the button lands on the pipelines tab"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.pipeline_branch.clone()),
        Some("main".into()),
        "the refs/heads/ spelling is stripped"
    );
    assert!(
        store.read_with(cx, |s, _| s.pipeline_watch_active()),
        "no run yet means the focus waits"
    );
    assert!(list_asks(&net_rx) >= 1, "the wait asks the runs list");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunsLoaded {
                folder: "/probe/alpha".into(),
                runs: Some(vec![build(48, "inProgress", None)]),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        !store.read_with(cx, |s, _| s.pipeline_watch_active()),
        "the first matching row ends the wait"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.id)),
        Some(48),
        "the merge-triggered run is the selection"
    );
    assert_eq!(
        detail_asks(&net_rx),
        [(48, false)],
        "and it is followed from its first answer"
    );

    cx.update_entity(&store, |s: &mut Store, cx| s.clear_pipeline_branch(cx));
    assert_eq!(
        store.read_with(cx, |s, _| s.pipeline_branch.clone()),
        None,
        "the chip is the way back to every branch"
    );
    assert_eq!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.id)),
        Some(48),
        "clearing the focus keeps the run open"
    );
}

/// The merge watch is about the run the merge started: an older run on the
/// same branch is not selected while waiting, and the first row newer than the
/// merge is.
#[gpui::test]
fn a_merge_watch_ignores_runs_from_before_the_merge(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "since");
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.azure_by_folder.insert(
            "/probe/alpha".to_string(),
            std::sync::Arc::new(AzureSnapshot {
                available: true,
                pipelines: vec![build(48, "completed", Some("succeeded"))],
                ..Default::default()
            }),
        );
        s.select_repo("/probe/alpha", cx);
    });
    let cx = open_window(&store, cx);
    let merged_at = chrono::Local::now();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.watch_pipelines_for("main".into(), Some(merged_at), false, cx)
    });
    draw(cx);
    assert!(
        store.read_with(cx, |s, _| s.open_run.is_none()),
        "the run from before the merge is not opened"
    );
    assert!(
        store.read_with(cx, |s, _| s.pipeline_watch_active()),
        "the focus waits for the merge-triggered run"
    );

    let mut merge_run = build(49, "inProgress", None);
    merge_run.at = Some(chrono::Local::now().to_rfc3339());
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::AzureRunsLoaded {
                folder: "/probe/alpha".into(),
                runs: Some(vec![merge_run, build(48, "completed", Some("succeeded"))]),
                budget: Default::default(),
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(
        store.read_with(cx, |s, _| s.open_run.as_ref().map(|open| open.run.id)),
        Some(49),
        "the first run newer than the merge is selected"
    );
}

/// A merged GitHub pull request draws the Pipelines verb in its head; the
/// button is the manual half of the Workflow setting.
#[gpui::test]
fn a_merged_pull_offers_the_pipelines_button(cx: &mut TestAppContext) {
    use warp_station_core::github::{GitHubItem, GitHubPull, GitHubSnapshot};

    let (store, _net_rx) = scratch_store(cx, "merged-pr");
    cx.update_entity(&store, |s: &mut Store, _| {
        s.update_settings(|settings| {
            settings.repos.enable_github = true;
            settings.repos.github_executable = std::env::current_exe()
                .expect("the test binary")
                .to_string_lossy()
                .to_string();
        });
        let item = GitHubItem {
            number: 9,
            title: "Merged".into(),
            is_pull_request: true,
            state: Some("MERGED".into()),
            base_ref_name: Some("main".into()),
            head_ref_name: Some("feat/flow".into()),
            ..Default::default()
        };
        s.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(GitHubSnapshot {
                available: true,
                pulls: vec![item.clone()],
                ..Default::default()
            }),
        );
    });
    let cx = open_window(&store, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo("/probe/alpha", cx);
        s.open_pull_request(
            GitHubItem {
                number: 9,
                title: "Merged".into(),
                is_pull_request: true,
                base_ref_name: Some("main".into()),
                ..Default::default()
            },
            cx,
        );
        s.on_net_event(
            UiEvent::GitHubPullLoaded {
                folder: "/probe/alpha".into(),
                number: 9,
                pull: Some(Box::new(GitHubPull {
                    number: 9,
                    title: "Merged".into(),
                    state: "MERGED".into(),
                    base_ref_name: "main".into(),
                    head_ref_name: "feat/flow".into(),
                    ..Default::default()
                })),
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("pull-pipelines").is_some(),
        "the merged pull request carries the Pipelines button"
    );
}

/// The post-push advance waits for the settled status: the push event itself
/// still carries the old ahead count, and the form should open against the
/// branch the probe now says is on the remote.
#[gpui::test]
fn a_push_advances_to_the_pull_form(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "advance");
    let folder = "/probe/alpha";
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| {
            settings.repos.enable_github = true;
            settings.repos.enable_azure_devops = false;
            settings.repos.github_executable = std::env::current_exe()
                .expect("the test binary")
                .to_string_lossy()
                .to_string();
            settings.workflow.auto_advance = true;
        });
        s.replace_repos(vec![entry("alpha", folder)]);
        s.repos[0].git_branch_name = Some("feat/flow".into());
        s.repos[0].git_to_push_count = 1;
        s.repos[0].git_branch_published = true;
        s.select_repo(folder, cx);
    });
    let cx = open_window(&store, cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::GitDone {
                folder: folder.into(),
                label: "push",
                error: None,
                commit_hash: None,
                detail: None,
            },
            cx,
        );
    });
    draw(cx);
    assert!(
        store.read_with(cx, |s, _| s.new_pull.is_none()),
        "the form waits for the settled status"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![warp_station_core::repo_store::StatusProbe {
                    folder_path: folder.into(),
                    branch_name: Some("feat/flow".into()),
                    modified_count: 0,
                    ahead_count: 0,
                    behind_count: 0,
                    last_commit_at: None,
                    last_fetch_at: None,
                    has_remote: true,
                    branch_on_remote: true,
                    github_web_url: Some("https://github.com/o/alpha".into()),
                }],
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(
        store.read_with(cx, |s, _| s.active_detail_tab()),
        WorkspaceTab::PullRequests,
        "the advance lands on the pull request tab"
    );
    assert!(
        store.read_with(cx, |s, _| s.new_pull.is_some()),
        "with the form open on the pushed branch"
    );
}
