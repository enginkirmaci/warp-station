# Dev Tools

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cd rust
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives in `~/.devtools` (settings, repos cache, notes, opencode
prompts). Linux install / AppImage:

```sh
rust/packaging/linux/install.sh install rust/target/release/devtools
rust/packaging/linux/build-appimage.sh          # → rust/packaging/linux/portable/DevTools-<v>-x86_64.AppImage
```

Windows: `rust/packaging/windows/build-portable.ps1` (+ Inno Setup script
`devtools.iss`). The opencode AI wands need `opencode` on PATH and the
OpenCode section enabled in Settings.
