#!/usr/bin/env bash
# Packs MarkdownViewerKit.Avalonia into packaging/nuget and verifies the payload.
# MARKDOWNVIEWERKIT_VERSION overrides the csproj version (used by the publish workflow).
set -euo pipefail
cd "$(dirname "$0")/../.."

CSPROJECT="src/MarkdownViewerKit/MarkdownViewerKit.Avalonia.csproj"
OUT="packaging/nuget"

VERSION="${MARKDOWNVIEWERKIT_VERSION:-$(grep -oP '(?<=<Version>)[^<]+' "$CSPROJECT" | head -1)}"
if [ -z "$VERSION" ]; then
	echo "No <Version> found in $CSPROJECT" >&2
	exit 1
fi

dotnet pack "$CSPROJECT" -c Release -o "$OUT" -p:Version="$VERSION"

NUPKG="$OUT/MarkdownViewerKit.Avalonia.$VERSION.nupkg"
SNUPKG="$OUT/MarkdownViewerKit.Avalonia.$VERSION.snupkg"
[ -f "$NUPKG" ] || { echo "Missing $NUPKG" >&2; exit 1; }
[ -f "$SNUPKG" ] || { echo "Missing $SNUPKG" >&2; exit 1; }

# Payload markers — the file mtime proves nothing (stale-staging trap), so check the
# assembly strings for the latest control surface and the embedded theme asset.
PAYLOAD=$(unzip -p "$NUPKG" lib/net8.0/MarkdownViewerKit.Avalonia.dll | strings)
echo "$PAYLOAD" | grep -q "InteractiveTaskLists" || { echo "dll payload check failed: InteractiveTaskLists missing" >&2; exit 1; }
echo "$PAYLOAD" | grep -qi "Themes/MarkdownTheme.axaml" || { echo "dll payload check failed: theme asset missing" >&2; exit 1; }
unzip -l "$NUPKG" | grep -q "README.md" || { echo "README.md missing from package" >&2; exit 1; }

echo "Packed MarkdownViewerKit.Avalonia $VERSION:"
ls -la "$OUT"
