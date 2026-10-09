#Requires -Version 5.1
# Builds DevTools for Windows and packages the release exe.
#   .\build.ps1             -> release build + portable zip in out\
#   .\build.ps1 -Installer  -> also compile the Inno Setup installer (ISCC.exe on PATH)
# Run from a Windows machine — GPUI has no Linux->Windows cross build.
param(
    [switch]$Installer
)
$ErrorActionPreference = 'Stop'

$here   = $PSScriptRoot
$repo   = Split-Path -Parent $here
$outDir = Join-Path $here 'out'

Push-Location $repo
try {
    cargo build --release -p devtools
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    $meta = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    $version = ($meta.packages | Where-Object { $_.name -eq 'devtools' }).version
} finally {
    Pop-Location
}

$exe = Join-Path $repo 'target\release\devtools.exe'
if (-not (Test-Path $exe)) { throw "release exe not found: $exe" }

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$zip = Join-Path $outDir "DevTools-$version-win-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
# stage exe + build-id so installed copies stay identifiable
$stage = Join-Path $outDir 'DevTools'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item $exe "$stage\devtools.exe"
"rust-devtools $version $(Get-Date -AsUTC -Format 'yyyy-MM-ddTHH:mm:ssZ')" |
    Out-File "$stage\build-id.txt" -Encoding ascii
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "portable: $zip"

if ($Installer) {
    $iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if (-not $iscc) { throw "Inno Setup compiler not found on PATH (install Inno Setup 6)" }
    & $iscc "/DMyAppVersion=$version" (Join-Path $here 'devtools.iss')
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
}
