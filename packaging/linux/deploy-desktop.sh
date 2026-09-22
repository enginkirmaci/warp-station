#!/usr/bin/env bash
#
# deploy-desktop.sh — build the Tools AppImage (via build-appimage.sh) and
# register it as a desktop application so it shows up in the system launcher
# (Omarchy app library, wofi/fuzzel, GNOME, KDE, ...). Installs, per-user
# (no sudo):
#   ~/Applications/Tools.AppImage                           stable-name copy
#   ~/.local/share/icons/hicolor/scalable/apps/devtools.png extracted from the AppImage
#   ~/.local/share/applications/tools.desktop               launcher entry
#
# Usage:
#   packaging/linux/deploy-desktop.sh                       # build 0.0.0-dev, then deploy
#   packaging/linux/deploy-desktop.sh 1.2.3                 # build 1.2.3, then deploy
#   packaging/linux/deploy-desktop.sh --version 1.2.3
#   packaging/linux/deploy-desktop.sh remove
#
# APPIMAGE_INSTALL_DIR overrides the AppImage target directory (default
# ~/Applications). Launching the AppImage needs FUSE2 (fuse2 on Arch/CachyOS);
# when it is missing, the entry falls back to --appimage-extract-and-run.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." &>/dev/null && pwd)"

DESKTOP_ID="tools"
# Unique icon id: "tools" collides with system themes (Yaru ships a legacy
# tools.svg that wins the lookup before user hicolor is consulted).
ICON_NAME="devtools"
# Must be a subdir DECLARED in hicolor's index.theme (1024x1024 is not one,
# and undeclared dirs are invisible to icon lookups).
ICON_SUBDIR="scalable"
APP_DIR="${APPIMAGE_INSTALL_DIR:-$HOME/Applications}"
APPIMAGE_DEST="$APP_DIR/Tools.AppImage"
ICON_DEST="$HOME/.local/share/icons/hicolor/$ICON_SUBDIR/apps/$ICON_NAME.png"
DESKTOP_DEST="$HOME/.local/share/applications/$DESKTOP_ID.desktop"

MODE="deploy"
VERSION="0.0.0-dev"
while [[ $# -gt 0 ]]; do
	case "$1" in
		remove)
			MODE="remove"
			shift
			;;
		--version)
			VERSION="${2:?--version requires a value}"
			shift 2
			;;
		--version=*)
			VERSION="${1#*=}"
			shift
			;;
		-*)
			echo "::error::unknown argument: $1" >&2
			exit 2
			;;
		*)
			VERSION="$1"
			shift
			;;
	esac
done

if [[ "$MODE" == "remove" ]]; then
	rm -f "$DESKTOP_DEST" "$ICON_DEST" "$APPIMAGE_DEST"
	update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
	echo ":: removed $DESKTOP_DEST"
	echo ":: removed $ICON_DEST"
	echo ":: removed $APPIMAGE_DEST"
	exit 0
fi

# Build first; the build script owns version validation, publish and staging.
# It names its output deterministically, so the deploy target below needs no
# "newest file" guessing.
"$SCRIPT_DIR/build-appimage.sh" --version "$VERSION"

APPIMAGE_SRC="$REPO_ROOT/packaging/linux/portable/Tools-$VERSION-x86_64.AppImage"
if [[ ! -f "$APPIMAGE_SRC" ]]; then
	echo "::error::AppImage not found after build: $APPIMAGE_SRC" >&2
	exit 1
fi

# 1. AppImage under a version-stable name, so the desktop entry's Exec
#    survives rebuilds.
mkdir -p "$APP_DIR"
install -m 755 "$APPIMAGE_SRC" "$APPIMAGE_DEST"
echo ":: installed $APPIMAGE_DEST"

# 2. Icon, extracted from the installed AppImage so it always matches the
#    shipped build. --appimage-extract writes squashfs-root/ into the CWD,
#    hence the scratch dir.
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
( cd "$SCRATCH" && "$APPIMAGE_DEST" --appimage-extract "$ICON_NAME.png" >/dev/null )
mkdir -p "$(dirname "$ICON_DEST")"
install -m 644 "$SCRATCH/squashfs-root/$ICON_NAME.png" "$ICON_DEST"
# Deploys older than the devtools/scalable move left colliding or undeclared
# copies behind (tools.png and devtools.png under 1024x1024).
rm -f "$HOME/.local/share/icons/hicolor/1024x1024/apps/tools.png" \
	"$HOME/.local/share/icons/hicolor/1024x1024/apps/devtools.png"
# GTK/Qt only read the theme cache; without a refresh the new icon is
# invisible to lookups. User hicolor has no index.theme, so regeneration
# fails there — removing the stale cache makes GTK scan the directory again.
if ! gtk-update-icon-cache -f -q "$HOME/.local/share/icons/hicolor" 2>/dev/null; then
	rm -f "$HOME/.local/share/icons/hicolor/icon-theme.cache"
fi
echo ":: installed $ICON_DEST"

# 3. Launcher entry. Absolute Exec; appends the FUSE-free extraction mode only
#    when FUSE2 is not installed. (No grep -q here: under pipefail, -q's early
#    exit SIGPIPEs ldconfig into rc 141.)
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
echo ":: DevTools is deployed — it now shows up in the launcher (::app-library / wofi)."
