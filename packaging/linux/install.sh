#!/usr/bin/env bash
# Builds DevTools (release) and installs the runnable file + desktop entry
# + hicolor icons so the Linux taskbar/launcher shows the app icon (matched
# via StartupWMClass). The staged binary in out/ is the deliverable and what
# the desktop entry launches - no zip, no installer.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"   # script lives in packaging/linux
out_dir="$here/out"

# snapit is a Windows-only zone snapper (cfg(windows) internals) and the
# Tools.SnapIt companion is .NET - the Linux deliverable is devtools alone.
(cd "$repo" && cargo build --release -p devtools)
# cargo may place artifacts off-repo (e.g. CARGO_TARGET_DIR on a fast
# local drive while the repo lives elsewhere)
target_dir="$(cd "$repo" && cargo metadata --no-deps --format-version 1 | jq -r '.target_directory')"

bin="$target_dir/release/devtools"
if [ ! -x "$bin" ]; then
	echo "release binary not found: $bin" >&2
	exit 1
fi

mkdir -p "$out_dir"
rm -rf "${out_dir:?}"/*
cp "$bin" "$out_dir/devtools"

icon_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
for f in "$here"/icons/devtools-*.png; do
	size="$(basename "$f" .png)"; size="${size#devtools-}"
	dir="$icon_root/${size}x${size}/apps"
	mkdir -p "$dir"
	cp "$f" "$dir/devtools.png"
done
# vector icon: launchers prefer scalable over upscaling the raster sizes
install -Dm644 "$here/../../crates/devtools/assets/icons/logo.svg" \
	"$icon_root/scalable/apps/devtools.svg"

app_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$app_dir"
cp "$here/devtools.desktop" "$app_dir/"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$app_dir" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -tf "$icon_root" || true
echo "built + installed: $out_dir/devtools, $app_dir/devtools.desktop + hicolor icons"
