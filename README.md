# Dev Tools

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives in `~/.devtools` (settings, repos cache, notes, opencode
prompts).

## warp-agent integration

The AI surface (commit-message / README wands, the daily summary, the model
catalog) runs in [warp-agent](../warp-agent) over its loopback door
(`~/.warp-agent/ipc.port` + `ipc.token`) — DevTools no longer spawns
`opencode serve` itself. Rules: a missing warp-agent binary turns the AI
features off; a merely closed one is launched on demand. Wand runs happen as
visible warp-agent chats in the repo's workspace (added when missing), the
daily summary runs in warp-agent's General workspace, and tool-permission
asks are answered in warp-agent's own UI.

## Package

- Linux: `packaging/linux/install.sh` installs the desktop entry + hicolor
  icons so the launcher shows the app; `Exec` points at
  `target/debug/devtools`.
- Windows (build on a Windows box — GPUI has no Linux→Windows cross build):
  `packaging/windows/build.ps1` stages the runnable files under
  `packaging/windows/out`: `devtools.exe`, the self-contained single-file
  `Tools.SnapIt.exe` companion and SnapIt's default settings. The app icon is
  embedded into the exe by `build.rs` from `packaging/windows/devtools.ico`.
