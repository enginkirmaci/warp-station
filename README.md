# Dev Tools

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives in `~/.devtools` (settings, repos cache, notes, opencode
prompts). The opencode AI wands need `opencode` on PATH and the OpenCode
section enabled in Settings.

## Package

- Linux: `packaging/linux/install.sh` installs the desktop entry + hicolor
  icons so the launcher shows the app; `Exec` points at
  `target/debug/devtools`.
- Windows (build on a Windows box — GPUI has no Linux→Windows cross build):
  `packaging/windows/build.ps1` produces a portable zip under
  `packaging/windows/out`; `-Installer` also compiles the Inno Setup
  setup.exe. The app icon is embedded into the exe by `build.rs` from
  `packaging/windows/devtools.ico`.
