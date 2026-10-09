#!/usr/bin/env bash
#
# build-appimage.sh — self-contained Linux AppImage of the Rust DevTools app.
#
# Usage:
#   packaging/linux/build-appimage.sh              # version 0.0.0-dev
#   packaging/linux/build-appimage.sh 1.2.3
#
# Output: packaging/linux/portable/DevTools-<version>-x86_64.AppImage
# Launching needs FUSE2; without it run with --appimage-extract-and-run.
# Set APPIMAGETOOL=/path/to/appimagetool to override the tool lookup.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

VERSION="0.0.0-dev"
if [[ "${1:-}" == "--version" ]]; then
	VERSION="${2:?--version requires a value}"
elif [[ "${1:-}" != "" ]]; then
	VERSION="$1"
fi

OUT_DIR="$SCRIPT_DIR/portable"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "==> cargo build --release"
cargo build --release --manifest-path Cargo.toml -p devtools
BIN="target/release/devtools"
[[ -x "$BIN" ]] || { echo "binary missing: $BIN" >&2; exit 1; }

# payload marker for the stale-payload trap: strings-verifiable build identity
PAYLOAD_ID="rust-devtools ${VERSION} $(date -u +%Y-%m-%dT%H:%M:%SZ)"

mkdir -p "$STAGE/usr/bin" "$STAGE/usr/share/metainfo" "$STAGE/usr/share/icons/hicolor"
install -m 0755 "$BIN" "$STAGE/usr/bin/devtools"
printf '%s\n' "$PAYLOAD_ID" > "$STAGE/usr/share/devtools-build-id.txt"

for size in 16 32 48 64 128 256; do
	dir="$STAGE/usr/share/icons/hicolor/${size}x${size}/apps"
	mkdir -p "$dir"
	install -m 0644 "$SCRIPT_DIR/devtools-${size}.png" "$dir/devtools.png"
done

sed "s|^Exec=.*|Exec=devtools|" "$SCRIPT_DIR/devtools.desktop" > "$STAGE/devtools.desktop"
# appimagetool resolves Icon= from the AppDir root
install -m 0644 "$SCRIPT_DIR/devtools-256.png" "$STAGE/devtools.png"
install -m 0644 "$SCRIPT_DIR/devtools-256.png" "$STAGE/.DirIcon"

cat > "$STAGE/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/devtools" "$@"
EOF
chmod +x "$STAGE/AppRun"

# find appimagetool: env override, system, else cached download
APPIMAGETOOL_BIN="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
if [[ -z "$APPIMAGETOOL_BIN" ]]; then
	TOOL_CACHE="$REPO_ROOT/build/tools"
	mkdir -p "$TOOL_CACHE"
	APPIMAGETOOL_BIN="$TOOL_CACHE/appimagetool-x86_64.AppImage"
	if [[ ! -x "$APPIMAGETOOL_BIN" ]]; then
		echo "==> downloading appimagetool"
		curl -fL --proto '=https' --tlsv1.2 \
			-o "$APPIMAGETOOL_BIN" \
			"https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
		chmod +x "$APPIMAGETOOL_BIN"
	fi
fi

mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/DevTools-${VERSION}-x86_64.AppImage"
ARCH=x86_64 "$APPIMAGETOOL_BIN" "$STAGE" "$OUT"

# stale-payload guard: extract the build-id and compare it (strings cannot
# see through the squashfs compression)
VERIFY_DIR="$(mktemp -d)"
if (cd "$VERIFY_DIR" && "$OUT" --appimage-extract "usr/share/devtools-build-id.txt" >/dev/null 2>&1) &&
	grep -q "rust-devtools ${VERSION}" "$VERIFY_DIR/squashfs-root/usr/share/devtools-build-id.txt" 2>/dev/null; then
	echo "payload verified: $PAYLOAD_ID"
else
	echo "payload marker missing — the AppImage may be stale" >&2
	rm -rf "$VERIFY_DIR"
	exit 1
fi
rm -rf "$VERIFY_DIR"

echo "built: $OUT"
ls -lh "$OUT"
