# Dev Tools

Compact developer utilities collection — Rust/GPUI app.

## Quick start

```sh
cargo run            # debug run
cargo test           # unit + live git parity + headless UI probes
```

User data lives in `~/.devtools` (settings, repos cache, notes, opencode
prompts). Linux install / AppImage:

```sh
packaging/linux/install.sh install target/release/devtools
packaging/linux/build-appimage.sh          # → packaging/linux/portable/DevTools-<v>-x86_64.AppImage
```

Windows: `packaging/windows/build-portable.ps1` (+ Inno Setup script
`devtools.iss`). The opencode AI wands need `opencode` on PATH and the
OpenCode section enabled in Settings.
