#!/usr/bin/env bash
#
# build-appimage.sh — generate a self-contained Linux (linux-x64) AppImage of the
# Tools GUI. The DevTools supervisor (src/DevTools) is Windows-only (named-pipe
# launcher + autostart), so the AppImage packages src/Tools only and AppRun
# execs the Tools binary directly.
#
# Usage:
#   packaging/linux/build-appimage.sh                # version defaults to 0.0.0-dev
#   packaging/linux/build-appimage.sh 1.2.3          # version = 1.2.3
#   packaging/linux/build-appimage.sh --version 1.2.3
#
# Output: portable/Tools-<version>-x86_64.AppImage
# Self-contained (no .NET runtime needed on the target); user data lives in
# ~/.devtools, seeded from the bundled settings/ tree, same as Windows. Launching
# the AppImage needs FUSE2 (fuse2/libfuse2 package); without it, run it with
# --appimage-extract-and-run. Set APPIMAGETOOL=/path/to/appimagetool to override
# the tool lookup (system appimagetool, else a cached download into build/tools).

set -euo pipefail

# --- Locate repo root (script lives in packaging/linux) ---
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." &>/dev/null && pwd)"
cd "$REPO_ROOT"

VERSION="0.0.0-dev"

# --- Parse args (same shape as packaging/windows/build-portable.sh) ---
while [[ $# -gt 0 ]]; do
	case "$1" in
		--version)
			VERSION="${2:?--version requires a value}"
			shift 2
			;;
		--version=*)
			VERSION="${1#*=}"
			shift
			;;
		-h|--help)
			sed -n '2,17p' "$0"
			exit 0
			;;
		*)
			# Treat a bare positional as the version (e.g. build-appimage.sh 1.2.3)
			if [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9] ]]; then
				VERSION="$1"
				shift
			else
				echo "::error::unknown argument: $1" >&2
				exit 2
			fi
			;;
	esac
done

# Validate version shape (same regex as the CI workflow).
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]]; then
	echo "::error::version '$VERSION' is not in x.y.z format" >&2
	exit 1
fi

# --- Prereqs ---
if ! command -v dotnet >/dev/null 2>&1; then
	echo "::error::dotnet SDK not found on PATH. Install the .NET 10 SDK." >&2
	exit 1
fi
if ! dotnet --version 2>/dev/null | grep -q '^10\.'; then
	echo "::error::this project needs the .NET 10 SDK (got '$(dotnet --version)')." >&2
	exit 1
fi

echo "::group::Resolve version"
echo "Version: $VERSION"
echo "::endgroup::"

# --- Paths ---
PUBLISH_DIR="build/appimage/linux-x64/publish"
APPDIR="build/appimage/AppDir"
OUT_DIR="portable"
APPIMAGE="$OUT_DIR/Tools-$VERSION-x86_64.AppImage"
TOOL_DIR="build/tools"
APPIMAGETOOL_IMG="$TOOL_DIR/appimagetool-x86_64.AppImage"

echo "::group::Publish Tools (linux-x64, self-contained)"
dotnet publish src/Tools/Tools.csproj \
	-c Release \
	-r linux-x64 \
	--self-contained true \
	-p:Version="$VERSION" \
	-o "$PUBLISH_DIR"
echo "::endgroup::"

echo "::group::Stage AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
# Same exclusions as the Windows portable staging (no pdb/xml in artifacts).
if command -v rsync >/dev/null 2>&1; then
	rsync -a \
		--exclude='*.pdb' \
		--exclude='*.xml' \
		"$PUBLISH_DIR/" "$APPDIR/usr/bin/"
else
	cp -a "$PUBLISH_DIR/." "$APPDIR/usr/bin/"
	find "$APPDIR/usr/bin" -type f \( -name '*.pdb' -o -name '*.xml' \) -delete
fi
if [[ ! -x "$APPDIR/usr/bin/Tools" ]]; then
	echo "::error::expected $APPDIR/usr/bin/Tools after publish, not found." >&2
	exit 1
fi
# Icon + desktop entry (.DirIcon is the AppImage-convention root icon; Icon=
# in the desktop entry resolves against the AppDir root).
cp "$SCRIPT_DIR/tools.desktop" "$APPDIR/tools.desktop"
cp src/Tools/Assets/logo.png "$APPDIR/tools.png"
ln -sfn tools.png "$APPDIR/.DirIcon"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/Tools" "$@"
RUN
chmod +x "$APPDIR/AppRun"
echo "::endgroup::"

echo "::group::Prepare appimagetool"
# Order: $APPIMAGETOOL env override -> system appimagetool -> cached download
# from the AppImage project's continuous release (bundles mksquashfs, so no
# squashfs-tools needed either). The downloaded tool is run with
# --appimage-extract-and-run so it works without FUSE.
APPIMAGETOOL_BIN=""
if [[ -n "${APPIMAGETOOL:-}" ]]; then
	APPIMAGETOOL_BIN="$APPIMAGETOOL"
elif command -v appimagetool >/dev/null 2>&1; then
	APPIMAGETOOL_BIN="$(command -v appimagetool)"
else
	mkdir -p "$TOOL_DIR"
	if [[ ! -f "$APPIMAGETOOL_IMG" ]]; then
		echo "Downloading appimagetool (one-time, cached in $TOOL_DIR)..."
		URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
		if command -v curl >/dev/null 2>&1; then
			curl -fL --retry 3 -o "$APPIMAGETOOL_IMG" "$URL"
		elif command -v wget >/dev/null 2>&1; then
			wget -q --tries=3 -O "$APPIMAGETOOL_IMG" "$URL"
		else
			echo "::error::neither curl nor wget found; cannot download appimagetool." >&2
			exit 1
		fi
	fi
	chmod +x "$APPIMAGETOOL_IMG"
	APPIMAGETOOL_BIN="$APPIMAGETOOL_IMG"
fi
if [[ "$APPIMAGETOOL_BIN" == *.AppImage ]]; then
	APPIMAGETOOL_RUN=("$APPIMAGETOOL_BIN" --appimage-extract-and-run)
else
	APPIMAGETOOL_RUN=("$APPIMAGETOOL_BIN")
fi
echo "appimagetool: ${APPIMAGETOOL_RUN[*]}"
echo "::endgroup::"

echo "::group::Build AppImage"
mkdir -p "$OUT_DIR"
rm -f "$APPIMAGE"
# Absolute paths: the --appimage-extract-and-run wrapper does not resolve
# relative paths against the caller's cwd.
"${APPIMAGETOOL_RUN[@]}" "$REPO_ROOT/$APPDIR" "$REPO_ROOT/$APPIMAGE"
echo "::endgroup::"

echo
echo ":: AppImage ready ::"
echo "  $REPO_ROOT/$APPIMAGE"
echo "  ($(du -h "$REPO_ROOT/$APPIMAGE" | cut -f1))"
