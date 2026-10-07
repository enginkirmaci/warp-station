#!/usr/bin/env bash
#
# install.sh — user-level Linux install of the Rust DevTools binary.
# Installs the binary into ~/.local/bin, the icon set into
# ~/.local/share/icons/hicolor and the desktop entry into
# ~/.local/share/applications.
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

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 0755 "$BINARY" "$HOME/.local/bin/devtools"

for size in 16 32 48 64 128 256; do
	dir="$HOME/.local/share/icons/hicolor/${size}x${size}/apps"
	mkdir -p "$dir"
	install -m 0644 "$SCRIPT_DIR/devtools-${size}.png" "$dir/devtools.png"
done

# Exec must resolve without a shell PATH for some launchers
sed "s|^Exec=.*|Exec=$HOME/.local/bin/devtools|" \
	"$SCRIPT_DIR/devtools.desktop" > "$HOME/.local/share/applications/devtools.desktop"

update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "installed: ~/.local/bin/devtools (launch with 'devtools' or from the app launcher)"
