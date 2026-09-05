# Packaging

Per-OS packaging scripts and assets. Scripts locate the repo root themselves,
so they can be run from anywhere. Artifacts land in `portable/` (portable zip,
AppImage) and `installer/` (Windows setup exe); both are gitignored.

## Windows

- `windows/setup.iss` — Inno Setup installer script (used by
  `.github/workflows/build-installer.yml`, which also builds the portable zip)
- `windows/build-portable.ps1` — portable zip from a Windows dev box
- `windows/build-portable.sh` — portable zip cross-built from a Linux machine

Windows artifacts ship BOTH apps: `DevTools.exe` (supervisor: named-pipe server
that launches the GUI) and `bin/Tools.exe` (the Avalonia GUI).

## Linux

- `linux/build-appimage.sh` — self-contained linux-x64 AppImage
- `linux/tools.desktop` — desktop entry bundled into the AppImage

```sh
packaging/linux/build-appimage.sh            # portable/Tools-0.0.0-dev-x86_64.AppImage
packaging/linux/build-appimage.sh 1.2.3
```

Packages ONLY the `Tools` GUI: the `DevTools` supervisor is Windows-only (named
pipes + autostart) and not needed here — `AppRun` execs `usr/bin/Tools`
directly. User data lives in `~/.devtools`, seeded from the bundled `settings/`
tree. Launching the produced AppImage needs FUSE2 (`fuse2` on Arch/CachyOS);
without it: `./Tools-*.AppImage --appimage-extract-and-run`.
