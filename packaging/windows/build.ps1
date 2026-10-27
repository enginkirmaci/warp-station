#Requires -Version 5.1
# Builds DevTools for Windows and stages the runnable files in out\:
# devtools.exe, Tools.SnapIt.exe (self-contained single file) and SnapIt's
# default settings. No zip, no installer - out\ is the deliverable.
# Run from a Windows machine - GPUI has no Linux->Windows cross build.
# Requires the .NET 10 SDK on PATH (builds the SnapIt companion).
$ErrorActionPreference = 'Stop'

$here   = $PSScriptRoot
$repo   = Split-Path -Parent (Split-Path -Parent $here)   # script lives in packaging\windows
$outDir = Join-Path $here 'out'

if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
    throw "dotnet not found on PATH - the SnapIt companion needs the .NET 10 SDK"
}

Push-Location $repo
try {
    cargo build --release -p devtools
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    # cargo may place artifacts off-repo (e.g. CARGO_TARGET_DIR on a fast
    # local drive while the repo lives on a network share)
    $targetDir = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).target_directory
} finally {
    Pop-Location
}

$exe = Join-Path $targetDir 'release\devtools.exe'
if (-not (Test-Path $exe)) { throw "release exe not found: $exe" }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Get-ChildItem $outDir -Force | Remove-Item -Recurse -Force

Copy-Item $exe (Join-Path $outDir 'devtools.exe')

# SnapIt companion: self-contained single-file publish - one exe with no
# runtime install and no loose native dlls (they self-extract at startup).
$publish = Join-Path $outDir '.snapit-publish'
dotnet publish (Join-Path $repo 'src\Tools.SnapIt\Tools.SnapIt.csproj') `
    -c Release -r win-x64 --self-contained true `
    -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true `
    -p:EnableCompressionInSingleFile=true -p:DebugType=None `
    -o $publish
if ($LASTEXITCODE -ne 0) { throw "dotnet publish (SnapIt companion) failed with exit code $LASTEXITCODE" }
Copy-Item (Join-Path $publish 'Tools.SnapIt.exe') $outDir
Copy-Item (Join-Path $publish 'settings') $outDir -Recurse
Remove-Item -Recurse -Force $publish

Write-Host "staged in $outDir"
