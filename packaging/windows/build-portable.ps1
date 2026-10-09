# build-portable.ps1 — release build + portable zip of the Rust DevTools app.
# Usage: powershell -File packaging\windows\build-portable.ps1 [-Version 1.2.3]
# Output: packaging\windows\portable\DevTools-<version>-win-x64.zip

param(
    [string]$Version = "0.0.0-dev"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $repoRoot

Write-Host "==> cargo build --release" -ForegroundColor Cyan
cargo build --release --manifest-path Cargo.toml -p devtools
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$bin = "target\release\devtools.exe"
if (-not (Test-Path $bin)) { throw "binary missing: $bin" }

$outDir = "$PSScriptRoot\portable"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$stage = Join-Path $outDir "DevTools"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage | Out-Null

Copy-Item $bin "$stage\devtools.exe"
"rust-devtools $Version $(Get-Date -AsUTC -Format 'yyyy-MM-ddTHH:mm:ssZ')" |
    Out-File "$stage\build-id.txt" -Encoding ascii

$zip = Join-Path $outDir "DevTools-$Version-win-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip

Write-Host "built: $zip" -ForegroundColor Green
Get-Item $zip | Select-Object Name, Length
