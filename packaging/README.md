# Packaging

Per-OS packaging scripts and assets. Scripts locate the repo root themselves,
so they can be run from anywhere. Everything flows through `packaging/`:
the Windows portable zip lands in `packaging/windows/portable/`, the Windows
installer in
`packaging/windows/installer/`, the Linux AppImage in
`packaging/linux/portable/` and NuGet packages in `packaging/nuget/`.
All of those are gitignored.

## Windows

- `windows/build-portable.ps1` — publish both apps for win-x64, stage the payload
  (`windows\stage`), verify it (both exes present, self-contained-sized) and zip it
  (`windows\portable\DevTools-Portable-<v>.zip`)
- `windows/setup.iss` — Inno Setup installer script; stages from the publish folders
  the script leaves behind and writes `windows\installer\`

```powershell
packaging\windows\build-portable.ps1                    # zip, version 0.0.0-dev
packaging\windows\build-portable.ps1 1.2.3
```

Windows artifacts ship BOTH apps: `DevTools.exe` (supervisor: named-pipe server
that launches the GUI) and `bin/Tools.exe` (the Avalonia GUI).

> **WARNING:** the `WINDOWS` compile constant follows the **build host** OS. Binaries
> published from a Linux host silently lack every `#if WINDOWS` feature (Ctrl+Shift+V
> password hotkey, registry autostart, named-pipe supervisor). Build Windows
> artifacts from a Windows machine (the scripts above) or CI
> (`.github/workflows/build-installer.yml`).

## Linux

- `linux/build-appimage.sh` — self-contained linux-x64 AppImage
- `linux/tools.desktop` — desktop entry bundled into the AppImage
- `linux/install-desktop.sh` — register the AppImage with the desktop
  launcher (Omarchy app library, wofi, GNOME, KDE, ...) per-user, no sudo

```sh
packaging/linux/build-appimage.sh            # packaging/linux/portable/Tools-0.0.0-dev-x86_64.AppImage
packaging/linux/build-appimage.sh 1.2.3
packaging/linux/install-desktop.sh           # newest AppImage in packaging/linux/portable/ → launcher
packaging/linux/install-desktop.sh packaging/linux/portable/Tools-1.2.3-x86_64.AppImage
packaging/linux/install-desktop.sh remove
```

`install-desktop.sh` copies the AppImage to `~/Applications/Tools.AppImage`
(override with `APPIMAGE_INSTALL_DIR`), extracts the icon into the user
hicolor icon theme and writes `~/.local/share/applications/tools.desktop`.
Re-running it after a rebuild refreshes all three in place.

Packages ONLY the `Tools` GUI: the `DevTools` supervisor is Windows-only (named
pipes + autostart) and not needed here — `AppRun` execs `usr/bin/Tools`
directly. User data lives in `~/.devtools`, seeded from the bundled `settings/`
tree. Launching the produced AppImage needs FUSE2 (`fuse2` on Arch/CachyOS);
without it: `./Tools-*.AppImage --appimage-extract-and-run` (the desktop
entry picks this fallback automatically).

Known gap: the Avalonia.Wayland backend (12.1.x) never sends
`xdg_toplevel.set_app_id`, so under a Wayland session the running window
reports an empty class/app_id — launchers and bars that match windows to
desktop entries by app_id can't link the running window to this entry. The
launcher entry itself is unaffected.

## NuGet

- `nuget/build-nupkg.sh` — packs `MarkdownViewerKit.Avalonia` into
  `packaging/nuget/` and verifies the payload (used by
  `.github/workflows/publish-nuget.yml` on `markdownviewerkit-v*` tags)
- `nuget/feed/` + the repo-root `nuget.config` — `Tools` consumes
  `MarkdownViewerKit.Avalonia` as a PACKAGE (not a ProjectReference) from this
  committed local feed. To publish it to nuget.org: add the `NUGET_API_KEY`
  repository secret, push the `markdownviewerkit-v0.1.0` tag and let
  `publish-nuget.yml` push it; afterwards the feed folder and the local source
  in `nuget.config` can be dropped.
