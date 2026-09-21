#!/usr/bin/env bash
#
# install-desktop.sh — register the Tools AppImage as a desktop application so
# it shows up in the system launcher (Omarchy app library, wofi/fuzzel, GNOME,
# KDE, ...). Installs, per-user (no sudo):
#   ~/Applications/Tools.AppImage                           stable-name copy
#   ~/.local/share/icons/hicolor/1024x1024/apps/tools.png   extracted from the AppImage
#   ~/.local/share/applications/tools.desktop               launcher entry
#
# Usage:
#   packaging/linux/install-desktop.sh                       # newest packaging/linux/portable/Tools-*.AppImage
#   packaging/linux/install-desktop.sh path/to/Tools-*.AppImage
#   packaging/linux/install-desktop.sh remove
#
# APPIMAGE_INSTALL_DIR overrides the AppImage target directory (default
# ~/Applications). Launching the AppImage needs FUSE2 (fuse2 on Arch/CachyOS);
# when it is missing, the entry falls back to --appimage-extract-and-run.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." &>/dev/null && pwd)"

DESKTOP_ID="tools"
ICON_NAME="tools"
ICON_SIZE_DIR="1024x1024"
APP_DIR="${APPIMAGE_INSTALL_DIR:-$HOME/Applications}"
APPIMAGE_DEST="$APP_DIR/Tools.AppImage"
ICON_DEST="$HOME/.local/share/icons/hicolor/$ICON_SIZE_DIR/apps/$ICON_NAME.png"
DESKTOP_DEST="$HOME/.local/share/applications/$DESKTOP_ID.desktop"

case "${1:-install}" in
	remove)
		rm -f "$DESKTOP_DEST" "$ICON_DEST" "$APPIMAGE_DEST"
		update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
		echo ":: removed $DESKTOP_DEST"
		echo ":: removed $ICON_DEST"
		echo ":: removed $APPIMAGE_DEST"
		;;

	install)
		# Pick the AppImage: explicit path, else the newest version in packaging/linux/portable/.
		if [[ $# -ge 2 ]]; then
			APPIMAGE_SRC="$2"
		else
			APPIMAGE_SRC="$(ls -1v "$REPO_ROOT"/packaging/linux/portable/Tools-*-x86_64.AppImage 2>/dev/null | tail -1 || true)"
			if [[ -z "$APPIMAGE_SRC" ]]; then
				echo "::error::no AppImage found in packaging/linux/portable/ — run packaging/linux/build-appimage.sh first." >&2
				exit 1
			fi
		fi
		if [[ ! -f "$APPIMAGE_SRC" ]]; then
			echo "::error::AppImage not found: $APPIMAGE_SRC" >&2
			exit 1
		fi

		# 1. AppImage under a version-stable name, so the desktop entry's Exec
		#    survives rebuilds.
		mkdir -p "$APP_DIR"
		install -m 755 "$APPIMAGE_SRC" "$APPIMAGE_DEST"
		echo ":: installed $APPIMAGE_DEST"

		# 2. Icon, extracted from the installed AppImage so it always matches
		#    the shipped build. --appimage-extract writes squashfs-root/ into
		#    the CWD, hence the scratch dir.
		SCRATCH="$(mktemp -d)"
		trap 'rm -rf "$SCRATCH"' EXIT
		( cd "$SCRATCH" && "$APPIMAGE_DEST" --appimage-extract "$ICON_NAME.png" >/dev/null )
		mkdir -p "$(dirname "$ICON_DEST")"
		install -m 644 "$SCRATCH/squashfs-root/$ICON_NAME.png" "$ICON_DEST"
		echo ":: installed $ICON_DEST"

		# 3. Launcher entry. Absolute Exec; appends the FUSE-free extraction
		#    mode only when FUSE2 is not installed. (No grep -q here: under
		#    pipefail, -q's early exit SIGPIPEs ldconfig into rc 141.)
		EXEC_ARGS=""
		if ! ldconfig -p 2>/dev/null | grep 'libfuse\.so\.2' >/dev/null; then
			EXEC_ARGS=" --appimage-extract-and-run"
			echo ":: fuse2 not found — Exec uses --appimage-extract-and-run"
		fi
		cat > "$DESKTOP_DEST" <<EOF
[Desktop Entry]
Type=Application
Name=DevTools
GenericName=Developer Tools
Comment=Compact developer utilities collection
Exec="$APPIMAGE_DEST"$EXEC_ARGS
Icon=$ICON_NAME
Terminal=false
Categories=Development;
StartupWMClass=Tools
EOF
		chmod 644 "$DESKTOP_DEST"
		echo ":: installed $DESKTOP_DEST"

		update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
		if command -v desktop-file-validate >/dev/null 2>&1; then
			desktop-file-validate "$DESKTOP_DEST" || true
		fi

		echo
		echo ":: DevTools is registered — it now shows up in the launcher (::app-library / wofi)."
		;;

	*)
		echo "::error::unknown argument: $1 (expected install [AppImage-path] or remove)" >&2
		exit 2
		;;
esac
