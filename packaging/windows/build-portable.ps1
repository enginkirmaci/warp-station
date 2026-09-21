#Requires -Version 5.1
<#
build-portable.ps1 — generate a self-contained Windows (win-x64) portable build:
publish DevTools (supervisor) and Tools (GUI), stage the payload, verify it and
zip it.

Output: packaging\windows\portable\DevTools-Portable-<version>.zip
The zip runs on a clean Windows box (no .NET install needed) and stores its
user data in %USERPROFILE%\.devtools, seeded from the bundled settings\ tree.

Usage:
    packaging\windows\build-portable.ps1                  # version defaults to 0.0.0-dev
    packaging\windows\build-portable.ps1 1.2.3            # version = 1.2.3
    packaging\windows\build-portable.ps1 -Version 1.2.3
    packaging\windows\build-portable.ps1 -Help
#>
[CmdletBinding()]
param(
    # Version in x.y.z form. A bare positional value is also accepted.
    [Parameter(Position = 0)]
    [string]$Version = "0.0.0-dev",

    [switch]$Help
)

$ErrorActionPreference = 'Stop'

# --- Helpers (write to stderr + non-zero exit, mirroring bash's `echo >&2; exit 1`) ---
function Fail([string]$Message) {
    [Console]::Error.WriteLine("::error::$Message")
    exit 1
}

function Show-Help {
    Write-Host @"
build-portable.ps1 — generate a self-contained Windows (win-x64) portable build
(packaging\windows\portable\DevTools-Portable-<version>.zip).

Usage:
    packaging\windows\build-portable.ps1                  # version defaults to 0.0.0-dev
    packaging\windows\build-portable.ps1 1.2.3
    packaging\windows\build-portable.ps1 -Version 1.2.3
    packaging\windows\build-portable.ps1 -Help
"@
}

# --- Locate repo root (this script lives in packaging\windows, but be defensive) ---
if ($Help) { Show-Help; exit 0 }
Set-Location (Join-Path $PSScriptRoot "..\..")

# Validate version shape (same regex as the CI workflow).
if ($Version -notmatch '^\d+\.\d+\.\d+') {
    Fail "version '$Version' is not in x.y.z format"
}

# --- Prereqs ---
if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
    Fail "dotnet SDK not found on PATH. Install the .NET 10 SDK."
}
$sdkVersion = (dotnet --version)
if ($sdkVersion -notmatch '^10\.') {
    Fail "this project needs the .NET 10 SDK (got '$sdkVersion')."
}

Write-Host "=== Resolve version ==="
Write-Host "Version: $Version"

# --- Paths (must match CI: split build output -> stage -> packaging\windows\portable) ---
# DevTools (launcher) and Tools (main app) publish to separate folders per the csproj
# OutputPath layout (DevTools -> build\, Tools -> build\bin\). The staged payload
# mirrors the installer: DevTools at the stage root, Tools under a bin\ subfolder.
$DevToolsPublish = "build\win-x64\publish"
$ToolsPublish    = "build\bin\win-x64\publish"
$StageDir        = "packaging\windows\stage"
$OutDir          = "packaging\windows\portable"
$Zip             = Join-Path $OutDir "DevTools-Portable-$Version.zip"

# Publish flags identical to .github/workflows/build-installer.yml.
$publishFlags = @(
    "-c", "Release",
    "-r", "win-x64",
    "--self-contained", "true",
    "-p:Platform=x64",
    "-p:Version=$Version",
    "-p:PublishSingleFile=true",
    "-p:IncludeNativeLibrariesForSelfExtract=true",
    "-p:EnableCompressionInSingleFile=true"
)

Write-Host "=== Restore dependencies ==="
dotnet restore Tools.slnx
if ($LASTEXITCODE -ne 0) { Fail "dotnet restore failed (exit $LASTEXITCODE)." }

Write-Host "=== Publish Tools ==="
dotnet publish src\Tools\Tools.csproj @publishFlags
if ($LASTEXITCODE -ne 0) { Fail "dotnet publish Tools failed (exit $LASTEXITCODE)." }

Write-Host "=== Publish DevTools ==="
dotnet publish src\DevTools\DevTools.csproj @publishFlags
if ($LASTEXITCODE -ne 0) { Fail "dotnet publish DevTools failed (exit $LASTEXITCODE)." }

# Sanity: each project publishes -r win-x64 into its own folder (verified at build
# time; these are the paths CI stages from). Make sure the expected exe exists in each.
foreach ($pair in @(
    @{ Exe = "DevTools.exe"; Dir = $DevToolsPublish },
    @{ Exe = "Tools.exe";    Dir = $ToolsPublish }
)) {
    $exePath = Join-Path $pair.Dir $pair.Exe
    if (-not (Test-Path $exePath)) {
        Fail "expected $exePath after publish, not found."
    }
}

Write-Host "=== Stage (exclude *.pdb, *.xml) ==="
if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
New-Item -ItemType Directory -Force -Path $StageDir, (Join-Path $StageDir "bin") | Out-Null
# Copy-then-delete keeps this script free of robocopy; the end state matches CI's
# `robocopy /E /XF *.pdb *.xml` (DevTools -> stage root, Tools -> stage\bin).
Copy-Item -Path (Join-Path $DevToolsPublish '*') -Destination $StageDir -Recurse
Copy-Item -Path (Join-Path $ToolsPublish '*') -Destination (Join-Path $StageDir "bin") -Recurse
Get-ChildItem -Path $StageDir -Recurse -File |
    Where-Object { $_.Extension -eq '.pdb' -or $_.Extension -eq '.xml' } |
    Remove-Item -Force

Write-Host "=== Verify staged payload ==="
# File mtimes prove nothing (stale-staging trap), so check the payload itself: the
# two executables must exist and be self-contained-sized.
foreach ($exe in @("DevTools.exe", "bin\Tools.exe")) {
    $exePath = Join-Path $StageDir $exe
    if (-not (Test-Path $exePath)) {
        Fail "staged payload check failed: $exe missing"
    }
    $len = (Get-Item $exePath).Length
    if ($len -lt 40MB) {
        Fail "staged payload check failed: $exe is only $len bytes (stale or thin publish?)"
    }
}

Write-Host "=== Create portable zip ==="
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (Test-Path $Zip) { Remove-Item -Force $Zip }
# A wildcard path puts the stage contents at the zip root (DevTools.exe at the
# root), matching the installer's {app} layout. CompressionLevel Optimal matches
# the CI workflow's Compress-Archive step.
Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $Zip -CompressionLevel Optimal

$zipFull = (Resolve-Path $Zip).Path
$sizeMB  = [math]::Round((Get-Item $Zip).Length / 1MB, 1)

Write-Host ""
Write-Host ":: Portable build ready ::"
Write-Host "  $zipFull"
Write-Host "  ($sizeMB MB)"
exit 0
