#!/usr/bin/env bash
#
# build-portable.sh — generate a self-contained Windows (win-x64) portable build
# from a Linux dev machine, mirroring the CI workflow in
# .github/workflows/build-installer.yml (publish + stage + zip steps).
#
# Usage:
#   ./build-portable.sh                # version defaults to 0.0.0-dev
#   ./build-portable.sh 1.2.3          # version = 1.2.3
#   ./build-portable.sh --version 1.2.3
#
# Output: portable/DevTools-Portable-<version>.zip
# The zip runs on a clean Windows box (no .NET install needed) and stores its
# user data in %USERPROFILE%\.devtools, seeded from the bundled settings\ tree.

set -euo pipefail

# --- Locate repo root (this script lives at the repo root, but be defensive) ---
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
cd "$SCRIPT_DIR"

VERSION="0.0.0-dev"

# --- Parse args ---
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
			sed -n '2,16p' "$0"
			exit 0
			;;
		*)
			# Treat a bare positional as the version (e.g. ./build-portable.sh 1.2.3)
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

# --- Paths (must match CI: split build output -> build/portable-stage -> portable/) ---
# DevTools (launcher) and Tools (main app) publish to separate folders per the csproj
# OutputPath layout (DevTools -> build/, Tools -> build/bin/). The portable zip mirrors
# the installer: DevTools at the zip root, Tools under a bin/ subfolder.
DEVTOOLS_PUBLISH="build/win-x64/publish"
TOOLS_PUBLISH="build/bin/win-x64/publish"
STAGE_DIR="build/portable-stage"
OUT_DIR="portable"
ZIP="$OUT_DIR/DevTools-Portable-$VERSION.zip"

# Publish flags identical to .github/workflows/build-installer.yml lines 54/57.
PUBLISH_FLAGS=(
	-c Release
	-r win-x64
	--self-contained true
	-p:Platform=x64
	-p:Version="$VERSION"
	-p:PublishSingleFile=true
	-p:IncludeNativeLibrariesForSelfExtract=true
	-p:EnableCompressionInSingleFile=true
)

echo "::group::Restore dependencies"
dotnet restore Tools.slnx
echo "::endgroup::"

echo "::group::Publish Tools"
dotnet publish src/Tools/Tools.csproj "${PUBLISH_FLAGS[@]}"
echo "::endgroup::"

echo "::group::Publish DevTools"
dotnet publish src/DevTools/DevTools.csproj "${PUBLISH_FLAGS[@]}"
echo "::endgroup::"

# Sanity: each project publishes -r win-x64 into its own folder (verified at build
# time; these are the paths CI stages from). Make sure the expected exe exists in each.
for pair in "DevTools.exe:$DEVTOOLS_PUBLISH" "Tools.exe:$TOOLS_PUBLISH"; do
	exe="${pair%%:*}"
	dir="${pair##*:}"
	if [[ ! -f "$dir/$exe" ]]; then
		echo "::error::expected $dir/$exe after publish, not found." >&2
		exit 1
	fi
done

echo "::group::Stage (exclude *.pdb, *.xml)"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR" "$STAGE_DIR/bin"
# rsync mirrors CI's `robocopy /E /XF *.pdb *.xml` (recursive copy, two excludes).
# DevTools -> stage root; Tools -> stage/bin (split layout, matching the installer).
if command -v rsync >/dev/null 2>&1; then
	rsync -a \
		--exclude='*.pdb' \
		--exclude='*.xml' \
		"$DEVTOOLS_PUBLISH/" "$STAGE_DIR/"
	rsync -a \
		--exclude='*.pdb' \
		--exclude='*.xml' \
		"$TOOLS_PUBLISH/" "$STAGE_DIR/bin/"
else
	# Fallback: cp everything, then delete the excluded file types per tree.
	cp -a "$DEVTOOLS_PUBLISH/." "$STAGE_DIR/"
	cp -a "$TOOLS_PUBLISH/." "$STAGE_DIR/bin/"
	find "$STAGE_DIR" -type f \( -name '*.pdb' -o -name '*.xml' \) -delete
fi
echo "::endgroup::"

echo "::group::Create portable zip"
mkdir -p "$OUT_DIR"
rm -f "$ZIP"
# Absolute output path: the python heredoc runs from the stage dir, so a
# relative path would resolve against the wrong cwd.
ABS_ZIP="$SCRIPT_DIR/$ZIP"
# Produce a real .zip that Windows Explorer opens natively. python is present
# on this dev box; fall back to `tar -a` if it's missing (tar auto-detects .zip).
if command -v python >/dev/null 2>&1; then
	(
		cd "$STAGE_DIR"
		# Compression level 6 ≈ PowerShell's CompressionLevel Optimal.
		python - "$ABS_ZIP" <<'PY'
import os, sys, zipfile
out = sys.argv[1]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
	for root, _dirs, files in os.walk("."):
		for name in files:
			p = os.path.join(root, name)
			arc = os.path.relpath(p, ".")
			z.write(p, arc)
PY
	)
else
	(cd "$STAGE_DIR" && tar -a -cf "$ABS_ZIP" .)
fi
echo "::endgroup::"

ABS_ZIP="$SCRIPT_DIR/$ZIP"
echo
echo ":: Portable build ready ::"
echo "  $SCRIPT_DIR/$ZIP"
echo "  ($(du -h "$SCRIPT_DIR/$ZIP" | cut -f1))"
