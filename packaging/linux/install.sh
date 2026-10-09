#!/usr/bin/env bash
#
# install.sh — user-level Linux install of the DevTools binary: the binary
# into ~/.local/bin, the hicolor icon set and the desktop entry into
# ~/.local/share.
#
# Usage: packaging/linux/install.sh install <path-to-devtools-binary>

set -euo pipefail

MODE="${1:-install}"
BINARY="${2:-}"

if [[ "$MODE" != "install" ]]; then
	echo "usage: install.sh install <path-to-devtools-binary>" >&2
	exit 1
fi
if [[ -z "$BINARY" || ! -x "$BINARY" ]]; then
	echo "usage: install.sh install <path-to-devtools-binary>" >&2
	exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

mkdir -p "$HOME/.local/bin"
install -m 0755 "$BINARY" "$HOME/.local/bin/devtools"

icon_root="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
for f in "$SCRIPT_DIR"/icons/devtools-*.png; do
	size="$(basename "$f" .png)"; size="${size#devtools-}"
	dir="$icon_root/${size}x${size}/apps"
	mkdir -p "$dir"
	install -m 0644 "$f" "$dir/devtools.png"
done

app_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$app_dir"
sed "s|^Exec=.*|Exec=$HOME/.local/bin/devtools|" \
	"$SCRIPT_DIR/devtools.desktop" > "$app_dir/devtools.desktop"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$app_dir" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t "$icon_root" || true

echo "installed: ~/.local/bin/devtools (launch with 'devtools' or from the app launcher)"
