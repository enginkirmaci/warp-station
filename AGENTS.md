# warp-station — agent instructions

Read [`../../AGENTS.md`](../../AGENTS.md) first. This file adds only the
app-specific facts. The user-facing description is in
[`README.md`](README.md); the deep UI guides are in
[`../../docs/agents/`](../../docs/agents/).

## Shape

Warp Station is the repository workbench: a GPUI app over a headless service
crate. It is two packages: the app at the repository root (`src/`, `tests/`,
root `Cargo.toml`) and the service crate under `crates/`:

| Package | Path | Role |
|---|---|---|
| `warp-station` | `.` (root package: `src/`, `tests/`) | lib + bin: store, net thread, UI |
| `warp-station-core` | `crates/warp-station-core/` | headless services, no gpui |

`warp-station-core` must not depend on gpui or gpui-kit. It takes
`warpui-kit-platform` with `default-features = false` so its tests stay
display-free.

## Where things live

- `src/main.rs` — the startup order; `src/lib.rs` is the boundary the probes
  build against.
- `src/store/` — the UI state: repos, selection, tabs, panes, notices and the
  event pump (`mod.rs`, `panes.rs`, `projection.rs`).
- `src/net.rs` — the net thread: its own tokio runtime and two async channels,
  commands in and `UiEvent` out.
- `src/ui/` — the window, rail, list, detail tabs, settings, dialogs, tools
  and the notes/pomodoro/SnapIt surfaces (`window.rs`, `repos.rs`,
  `repos_list.rs`, `changes.rs`, `pull_request.rs`, `azure_tab.rs`,
  `file_tree.rs`, `resize.rs`, `settings.rs`, `dialogs.rs`, `pomodoro.rs`,
  `windows_tools.rs`, …).
- `src/ipc.rs` — the single-instance door (`focus` only).
- `src/theme.rs`, `assets.rs`, `logging.rs`, `notices.rs`, `shortcuts.rs`.
- `crates/warp-station-core/src/` — `git/`, `activity.rs`, `azure.rs`,
  `github.rs`, `settings.rs`, `paths.rs`, `repo_store.rs`, `scan.rs`,
  `refresh.rs`, `snapit.rs`, `warp.rs`, `warp_notes.rs`, `pomodoro.rs`,
  `opencode.rs`, `autostart.rs`, `hotkey.rs`, `keepawake.rs`,
  `desktop_notifications.rs`, `jsonio.rs`, `exe.rs`, `process.rs`,
  `terminal.rs`, `flow.rs`, `worktree.rs`, and the rest.
- `tests/` — headless probes; `examples/diff_demo.rs` is a demo, not a test.

## Startup order

Keep this order; several steps depend on the previous one:

1. On Linux, a `--copy-clipboard-password` launch pings the running instance's
   net thread and exits **before** any logging or GPUI startup.
2. `ipc::try_forward()` — a live instance gets the launch and this process
   exits before migrations or logging touch the same files.
3. `warp_station_core::paths::migrate_legacy_data()`,
   `migrate_legacy_ai_data()` and `migrate_suite_layout()` — before logging,
   so the fresh `~/.warp/logs` never collides with an absorbed one.
4. `logging::initialize()`; keep the guard in `main`. `logging::install_panic_hook()`
   follows, so a panic logs its thread, location and backtrace: a desktop
   launch has no stderr to show it.
5. `application().run(...)`: net thread and `Store`, then
   `warpui_kit_core::bootstrap::init`, then the Windows auto-start blocks,
   the event pump, the once-per-second beat and the window.

## App-specific contracts

- **warp-agent client.** The AI surface (commit-message/README wands, daily
  summary, harness/model/variant catalog) runs through warp-agent's loopback
  door. Warp Station does not spawn `opencode serve` itself. A missing
  warp-agent binary disables the AI features; a closed one is launched on
  demand. Wand runs are real warp-agent chats in the repo workspace; the daily
  summary runs in the General workspace.
- **Worktrees.** The repo workspace's `Worktrees` tab lists the repository's
  linked checkouts (branch, path, uncommitted count, distance from upstream)
  through `warpui-kit-worktree` on the net thread; "Chat here" sends the door's
  `new-chat` with the checkout's folder, and removal/prune run the kit's git
  calls. A worktree is never a repo of its own: no row, tab or flow key, and
  Station's own Changes/History stay on the parent checkout.
- **Notes.** The notes surfaces launch warp-notes with `--store` (the resolved
  `notes_store_path`) and an explicit scope: the titlebar, the summary and the
  global-search note hits ask for `--root` (the whole store, never the
  selected repo), while a repo's launcher pins `--repo <name>`; a hit also
  passes `--note`. A missing binary shows a toast and nothing else breaks.
- **Pomodoro.** The rail's Pomodoro section is the live view of the sibling
  warp-pomodoro app: it reads `~/.warp/pomodoro/state.json` and, while the app
  runs, its token-authenticated door (`ipc.port` + `ipc.token`) with `status`,
  `start`, `pause`, `reset`, `skip` and `focus`. The page polls `status` once a
  second only while its section is on screen and falls back to the snapshot
  file when the door is down; a closed app reads as "Not running" with no
  error toast, and Start launches `warp-pomodoro --start`. The durations,
  long-break cycle, history and notifications live in the app's own settings —
  Station keeps only `pomodoro.enable_pomodoro` and `pomodoro.executable`.
  `WARP_STATION_POMODORO_DIR` redirects the folder for probes.
- **SnapIt.** Reads the same `~/.warp/snapit` files as warp-snapit. On Windows
  the auto-start setting launches the configured executable with
  `--parent <pid>`. An empty setting resolves to the packaged
  `warp-snapit.exe` next to the app; the retired .NET `Tools.SnapIt.exe` is
  never started.
- **Shared UI.** Text size and palette come from `~/.warp/ui.json` through
  `warpui-kit-core`; the app stores neither, and the accent follows the live
  palette like the other apps. Do not copy a fixed accent into a new app.
- **Layout.** Everything lives under `~/.warp/station` (versioned
  `settings.json` plus the disposable `cache/repos.json`,
  `cache/models.json` and `cache/flow.json`), except the shared
  `~/.warp/ui.json`, `~/.warp/logs`, the SnapIt settings at `~/.warp/snapit`
  and the notes content at `~/notes`.
- **Settings.** Repo tags live in `settings.json` (`repos.repo_tags`), not in
  the cache. The contract is `../../docs/agents/settings-schema.md`.
- **Folder groups.** `repos.group_repos_by_root` groups each repo under the
  folder `repos.group_root_depth` levels above it (1 = the folder containing
  it); scan roots only decide membership, never the header. A repo outside
  every root lands in "Other".
- **Automatic refresh.** `repos.auto_refresh_on_focus` (default on) rescans
  the list when the app regains focus; `repos.auto_refresh_minutes` (null/0 =
  off) rescans every N minutes. Both run only while a Warp Station window is
  focused and the once-a-second beat drives them through `Store::record_focus`
  / `Store::check_auto_refresh`. Each pass is the local scan plus
  `refresh_active_tab`: the selected repository's changes (Changes and the
  Overview read them), its history when History is up, and one repository's
  GitHub/Azure activity pass (plus the open pull/run detail) when a platform
  tab is active. The full-list eager activity set stays off this path, and no
  `git fetch` runs. A hidden repository tab reloads when it is selected.
- **Pipelines follow.** While the Pipelines tab is on screen and its run can
  still change, `Store::check_run_follow` (the once-a-second beat) re-reads the
  open run's detail — every 13 s on GitHub, 20 s on Azure — and asks the run
  list every 90 s. A finished run costs one last log call (GitHub only when it
  failed) and one list call, then the loop stops; a failed poll doubles the
  wait up to two minutes and keeps the detail already on screen.
  The request counts are measured, not one per poll: `gh run view --json` is 3
  core REST calls and `gh run list` 2; Azure's build record and timeline are
  one call each. Watching one run therefore costs about 150 GitHub or 67 Azure
  requests per ten minutes — roughly 18 % of GitHub's 5 000 an hour and 17 % of
  Azure DevOps' 200 TSTUs per five minutes. The cadence is the baseline: both
  platforms report `X-RateLimit-*`, and an answer that says the window is down
  to `RATE_RESERVE` parks the next poll until it refills (a 429's
  `Retry-After` included; Azure's are read straight off the response, GitHub's
  through the free `gh api rate_limit` probe when a call fails). Do not tighten
  the cadence or poll while the tab is hidden without redoing that budget;
  `follow_probe` drives the loop headlessly.
  A merged pull request's **Pipelines** verb sets `Store::pipeline_branch` to
  its merge target and either opens the newest run on that branch or waits for
  the run the merge starts. Only rows at or after the merge qualify (a minute
  of clock slack; no timestamp is not hidden), so an older run on the target
  branch is never mistaken for the merge's. The wait reads the runs list every
  15 s for up to five minutes (`PIPELINE_WATCH` — at most 20 list calls, then
  it stops). The branch focus is session-only and its chip clears it.
  `workflow.watch_in_background` is the one deliberate exception to hidden-tab
  polling: it continues the detail cadence of a run that was already being
  watched (and, while a merge watch still has no run, its bounded 15 s runs
  read), and the run-end toast is gated by `notifications.pipelines` (desktop
  only; the in-app toast always lands).
- **Flow strip.** `crates/warp-station-core/src/flow.rs` owns the disposable
  `cache/flow.json`; `Store::flow_view` turns the live pull/run snapshots and
  that record into the Changes strip's four steps, and `run_flow_action` is
  the only dispatcher. `watch_all_runs_on_branch` tallies the merge branch's
  runs from the lightweight list (never a second detail poll) and raises one
  "2 of 3 passed" gate toast; a gate suppresses the per-run toast. The New
  pull request form is now platform-neutral: GitHub goes through
  `gh pr create`, Azure through the REST POST (`azure::create_pull`), and the
  optional reviewer is passed through on GitHub and resolved to an Azure
  identity GUID before the POST. The form's reviewer field completes from the
  platform's people: `Store::new_pull_reviewers` returns GitHub's assignable
  logins (`github::assignees`, cached per folder, one `gh api .../assignees`
  read) or Azure's cached `azure_users` (the value is the unique name the
  create path resolves, the label the display name). Up/Down move the
  highlight, Enter takes it, a press takes the row, and Esc closes the list;
  the list is in flow under the field, not an overlay. The pipeline toast's
  button opens the run in the app; the desktop notification has no click
  callback on this path.
- **Keyboard and titles.** List keys belong to the focused pane (repo list,
  Changes files and the Merge Changes rail above them, History commits,
  PR/Issues, Pipelines runs, the PR/Azure Files and Commits rails, and the
  selected commit's own files inside a PR's Commits tab); the window delegates
  only while it owns the focus
  itself. Every list column also carries a drag bar on its right edge
  (`src/ui/resize.rs`, widths in `src/store/panes.rs`); the page list keeps
  `repos.list_width` and the workbench columns share
  `settings.layout.pane_widths`. `Enter` opens the selected repository as a tab, app launchers are
  `Shift`+letter (`Shift C` VS Code), and `Alt F`/`Alt A`/`Alt C` are
  favorite, archive/restore and Changes. `warpui_kit_core::pane_nav` carries
  `←`/`→` and `Tab`/`Shift+Tab` between the lists on screen, never wrapping;
  the page clears its order per render and each view registers only while it
  draws it. `?`/`Ctrl+/` and the titlebar icon open the shortcut sheet
  (`src/shortcuts.rs` builds it from the launcher list and the fixed table).
  The OS title carries `Store::activity()` next to the titlebar chip. That chip also opens the running panel: `Store::running_jobs`
  lists every live job (scan, per-repo git work, generation, summary, clones)
  in the order it started, and the panel closes when the last one lands. The
  Refresh button is a local scan plus the eager activity set; it never runs a
  `git fetch`. The family rules are
  `../../docs/agents/design-system.md` (`## Keyboard navigation`, `## Motion`).

## Tests

```sh
cargo test -p warp-station -p warp-station-core
cargo build -p warp-station
```

- Unit tests live beside the code; integration probes are under
  `tests/`. `changes_probe`, `repos_probe`,
  `settings_probe`, `split_probe`, `theme_probe`, `tools_probe` and the rest
  drive real views in headless windows.
- `changes_probe` and `git_parity` read this repository's own git history.
  Run them from a full clone, not a shallow one.
- Probes that write settings use a scratch `HOME`/`USERPROFILE` and a lock.
  `WARP_STATION_IPC_DIR` redirects the single-instance endpoint for probes.
- The env hooks (`WARP_STATION_PAGE`, `_SHEET`, `_SELECT`, `_TOOLS_OPEN`,
  `_GROUPS`, `_NOTES`, `_IPC_DIR`) are read once at window construction; see
  [`../../docs/agents/testing-and-captures.md`](../../docs/agents/testing-and-captures.md).

## Platform notes

- Linux: the `--copy-clipboard-password` hotkey payload; `notify-send` desktop
  notifications; FreeDesktop autostart.
- Windows: SnapIt auto-start, keep-awake, WinRT toasts, the installer and the
  generated asset table. The packaged exe hides the console with the
  `windows_subsystem` attribute; debug builds keep it.

## Do not

- Add `gpui`/`gpui-kit` to `warp-station-core`.
- Move `ipc::try_forward` after logging or migrations; a second launch would
  then write the same files.
- Change the door protocol without updating warp-agent in the same commit.
- Hand-roll the theme bootstrap or add an accent picker. Read the live palette
  and pass `current_ramp().accent_colors()`.
- Add a per-app build script or installer; the suite's `scripts/` owns those.

## Public history and secrets

- Never commit a token, PAT, password, private IP, Wi-Fi SSID, home path or
  personal email in file content. Use placeholders in docs and examples.
- Pass credentials through GitHub Actions secrets only; never echo or log
  them.
- Before publishing, force-pushing or adding a remote, run this over the
  current tree and every ref:

  ```sh
  git grep -n -i -E '192\.168\.[0-9]+\.[0-9]+|10\.[0-9]+\.[0-9]+\.[0-9]+|/home/[a-z]+|ghp_|gho_|github_pat_|ghs_' -- .
  ```
