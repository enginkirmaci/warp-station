# Dev Tools

Compact Avalonia developer utilities collection.

## Quick start
 
Build:
```sh
dotnet build Tools.slnx
```

Run (from IDE or CLI):
```sh
dotnet run --project src/Tools/Tools.csproj
```


## Installer (GitHub Actions)

This repository includes a GitHub Actions workflow that builds the application and generates a Windows installer using Inno Setup. The workflow is defined in `.github/workflows/build-installer.yml`.

How it works:
- Restores and builds the solution with .NET 10
- Publishes self-contained x64 builds of both `Tools` and `DevTools`
- Installs Inno Setup on the runner and compiles `setup.iss`
- Uploads the generated installer as a workflow artifact
- When triggered by a tag (for example `v1.0.0`) the workflow will also create a GitHub Release and attach the installer

`DevTools.exe` is the supervisor: it launches `Tools.exe` and hosts the named pipe server the GUI uses. The installer places both in the same folder and the Start Menu / Desktop shortcuts launch `DevTools.exe`, which starts `Tools.exe` automatically.

Triggering a release (example):

```sh
git tag v1.0.0
git push origin v1.0.0
```

You can also run the workflow manually from the Actions tab (workflow dispatch).

Notes:
- The installer is produced without code signing.
- The Inno Setup script is `packaging/windows/setup.iss` (moved out of the repo root; its source paths are relative to its own location).

## Packaging

Packaging scripts and assets live under `packaging/` — see `packaging/README.md`:

- `packaging/windows/` — Inno Setup script plus the portable-zip builders
  (`build-portable.ps1` for Windows, `build-portable.sh` to cross-build the
  Windows zip from Linux)
- `packaging/linux/` — `build-appimage.sh`, a self-contained Linux AppImage
  builder

Artifacts land in `portable/` (portable zip, AppImage) and `installer/`
(Windows setup exe). Both folders are gitignored.

## Linux (AppImage)

The Linux build packages only `Tools`: the `DevTools` supervisor is
Windows-only (named-pipe launcher + autostart), so Linux does not need it —
the AppImage's `AppRun` execs the `Tools` binary directly.

```sh
packaging/linux/build-appimage.sh            # portable/Tools-0.0.0-dev-x86_64.AppImage
packaging/linux/build-appimage.sh 1.2.3
```

The AppImage is self-contained (no .NET runtime needed on the target) and
stores user data in `~/.devtools`, seeded from the bundled `settings/` tree,
same as Windows. Launching it needs FUSE2 (`fuse2` on Arch/CachyOS); without
it, run `./Tools-*.AppImage --appimage-extract-and-run`.

## License

See repository LICENSE (MIT).