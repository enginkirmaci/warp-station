#!/usr/bin/env bash
# install the DevTools desktop entry + hicolor icons so the Linux
# taskbar/launcher shows the app icon (matched via StartupWMClass)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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
echo "installed: $app_dir/devtools.desktop + hicolor icons"
