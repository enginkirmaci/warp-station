# Warp Station

A repository workbench for Linux and Windows: every repository you work on in
one window — its changes, history, pull requests, pipelines and worktrees —
with the AI surface (commit messages, README drafts, a daily summary) running
in [Warp Agent](https://github.com/enginkirmaci/warp-agent) over its loopback
door.

Part of the [warp suite](https://github.com/enginkirmaci/warp-suite). Rust and
GPUI.

## Install

The normal way is the suite bundle, which installs every warp app at once:

- **Linux** — download `warp-suite-<version>-linux.tar.gz` from the
  [suite release](https://github.com/enginkirmaci/warp-suite/releases), extract
  it, and run `./install.sh`. Binaries go to `~/.local/bin`, desktop entries
  and icons to `~/.local/share`; everything is per-user, no root needed.
- **Windows** — download `warp-suite-<version>-windows-x64.zip`, extract it,
  and run `install.cmd`. It installs under
  `%LOCALAPPDATA%\Programs\warp-suite` with a Start Menu folder. The same ZIP
  is portable: run the executables under `apps\` without installing.

Warp Station also ships on its own: the Linux tarball and the Windows portable
zip and installer are on
[this repository's releases](https://github.com/enginkirmaci/warp-station/releases).

## Run

```sh
cargo run            # debug build
cargo test           # unit tests, live git parity checks and headless UI probes
```

The first build fetches GPUI from the zed git repository and takes a while. It
needs the usual Wayland/Vulkan development packages.

## What it does

- **Repositories** in a rail, each opening as a tab: Changes, History,
  Worktrees, pull requests, issues and pipelines, with a diff pane and a merge
  editor for a stopped merge, rebase, cherry-pick or revert.
- **The AI surface** — the commit-message and README wands, the daily summary,
  the harness/model/variant catalog — runs in Warp Agent. A missing Warp Agent
  binary turns those features off; a merely closed one is launched on demand.
  Wand runs appear as real Warp Agent chats in the repository's workspace.
- **The Pomodoro page** is a live view of the sibling
  [warp-pomodoro](https://github.com/enginkirmaci/warp-pomodoro) timer: the
  countdown, phase, cycle dots and the day's focus counts, with Start/Pause and
  Reset driving that app over its door. Its durations, history and
  notifications live there, never here.
- **Worktrees** for a repository are listed with their branch, path,
  uncommitted count and distance from upstream; a chat can be started inside
  one with a single door call.
- **Keyboard-first**: `?` opens the shortcut sheet, `Shift`+letter launches an
  app, and every list's right edge drags.

## Where it keeps its files

Everything lives under `~/.warp`:

| Path | What |
| --- | --- |
| `~/.warp/station/settings.json` | this app's versioned settings |
| `~/.warp/station/cache/` | the repository and model caches (disposable) |
| `~/.warp/station/assistant/` | the prompt templates |
| `~/.warp/logs/` | the suite's shared rolling logs |
| `~/.warp/snapit/` | the SnapIt settings and layouts, shared with warp-snapit |
| `~/notes` | the notes store, shared with warp-notes |

Repo tags live in `settings.json`, so the cache is disposable. A legacy
`~/.devtools` root is absorbed into `~/.warp` on first launch.

## Siblings

| App | What it adds |
| --- | --- |
| [warp-agent](https://github.com/enginkirmaci/warp-agent) | the chat client the AI surface drives |
| [warp-notes](https://github.com/enginkirmaci/warp-notes) | per-repo markdown notes |
| [warp-pomodoro](https://github.com/enginkirmaci/warp-pomodoro) | the focus timer behind the Pomodoro page |
| [warp-flow](https://github.com/enginkirmaci/warp-flow) | the workflow engine |
| [warp-snapit](https://github.com/enginkirmaci/warp-snapit) | the Windows zone snapper |

## Build and package

```sh
cargo build --release
```

The `release` workflow packages this repository alone: on Linux it stages the
binary, the desktop entry and the hicolor icons into a tarball; on Windows it
writes the portable zip and the Inno Setup installer. The assets stay here
under `packaging/linux/` and `packaging/windows/`.

## License

GPL-3.0. See [LICENSE](LICENSE).
