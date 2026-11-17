# Warp Station

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives under `~/.warp`: this app's own folder `~/.warp/station`
(versioned `settings.json`, `cache/repos.json`, `cache/models.json` and
`assistant/` with the prompt templates), the suite's shared `logs/`, the kit's
`ui.json` and the SnapIt settings. Repo tags live in `settings.json`, so the
cache is disposable. The notes store is user content in `~/notes`. An old
`opencode/` folder — or a stray `ai/` from a development build — is split on
startup, a legacy `~/.devtools` root is absorbed into `~/.warp` on first
launch, and the root files move into `~/.warp/station` once.

The rail's **Pomodoro** page is the live view of the sibling
[warp-pomodoro](../warp-pomodoro) timer: the countdown, phase, cycle dots and
the day's focus counts, with Start/Pause and Reset driving the app over its
loopback door and "Open Pomodoro" for everything else. Its durations,
long-break cycle, history and notifications live in that app; Station only
names the executable and can hide the section in Settings › Suite apps.

## Keyboard

`?` (or `Ctrl+/`, or the keyboard icon in the title bar) opens the shortcut
sheet: every key the window answers to, grouped by the surface that owns it.
`Esc` closes it.

`Ctrl+Shift+V` copies the stored Clipboard Password. The page that stores it
lives under Tools once **Settings › External tools › Enable Clipboard
Password** is on; with that switch on, the app binds the same shortcut
system-wide at the next launch.

On a focused repository row: `↑`/`↓` move, `Enter` opens the repository as a
tab, `Alt+F` favorites, `Alt+A` archives or restores, and `Alt+C` opens
Changes. `Shift`+letter opens an app (`Shift+C` is VS Code; the launch menu
shows the rest). `←`/`→` and `Tab`/`Shift+Tab` move the keyboard between the
lists on screen — the repository list, Changes files, History commits, the
PR/Issues list, Pipelines runs and the pull request's Files/Commits rails —
and stop at the end instead of wrapping.

## warp-agent integration

The AI surface (commit-message / README wands, the daily summary, the
harness/model/variant catalog) runs in [warp-agent](../warp-agent) over its
loopback door (`~/.warp/agent/ipc.port` + `ipc.token`) — Warp Station no longer
spawns `opencode serve` itself. Rules: a missing warp-agent binary turns the
AI features off; a merely closed one is launched on demand. Wand runs happen
as visible warp-agent chats in the repo's workspace (added when missing), the
daily summary runs in warp-agent's General workspace, and tool-permission
asks are answered in warp-agent's own UI. Settings picks the harness (the
door reports the options), one model and that model's variant; commit-message
generation uses the same selection as every other wand.

A repository's `Worktrees` tab lists the linked checkouts of that repository —
their branch, path, uncommitted count and distance from upstream — and starts a
chat inside one with a single door call, opens its folder, or removes it (a
dirty checkout asks first). Worktrees belong to their repository, so Station
never lists one as a repository of its own.

The switches are independent: **Enable warp-agent** arms the whole AI
surface (wands, summary, the catalog), while **Enable OpenCode** and
**Enable DeepSeek** only show the repo rows' CLI buttons, which launch the
`opencode` / `dsh` CLIs themselves in a terminal (`opencode` with
`--model <model>` when the opencode harness is selected; `dsh` boots its own
profile and takes no model) — that launch never goes through warp-agent, and
the AI features never need the CLI binaries.

## Package

The `release` workflow packages this repository alone:

- Linux: the tag job stages the binary, the desktop entry and hicolor icons
  into a tarball.
- Windows: the tag job writes the portable zip and the Inno Setup installer.
  The app icon is embedded into the exe by `build.rs` from
  `packaging/windows/warp-station.ico`.

A local build is `cargo build --release`. The assets stay here:
`packaging/linux/{*.desktop,icons}`, `packaging/windows/{*.ico,*.iss}`.

## SnapIt

The Windows zone snapper lives in its own repo,
[enginkirmaci/warp-snapit](https://github.com/enginkirmaci/warp-snapit), and
reads the same `~/.warp/snapit` settings/layouts. `warp-snapit.exe` is
distributed through [warp-suite](https://github.com/enginkirmaci/warp-suite)
releases (`snapit.executable` in the settings points Warp Station at the exe).
