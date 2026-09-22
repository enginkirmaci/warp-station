#Requires -Version 5.1
<#
deploy-desktop.ps1 — build the portable zip (via build-portable.ps1) and
deploy it per-user, no admin required:
  %LOCALAPPDATA%\Programs\DevTools              extracted payload
  %USERPROFILE%\AppData\Roaming\...\Start Menu\Programs\Dev Tools.lnk
User data lives in %USERPROFILE%\.devtools and is never touched by a deploy.

Usage:
    packaging\windows\deploy-desktop.ps1                  # build 0.0.0-dev, then deploy
    packaging\windows\deploy-desktop.ps1 1.2.3            # build 1.2.3, then deploy
    packaging\windows\deploy-desktop.ps1 -Version 1.2.3
    packaging\windows\deploy-desktop.ps1 -Remove
    packaging\windows\deploy-desktop.ps1 -Help
#>
[CmdletBinding()]
param(
    # Version in x.y.z form. A bare positional value is also accepted.
    [Parameter(Position = 0)]
    [string]$Version = "0.0.0-dev",

    [switch]$Remove,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'

function Fail([string]$Message) {
    [Console]::Error.WriteLine("::error::$Message")
    exit 1
}

function Show-Help {
    Write-Host @"
deploy-desktop.ps1 — build the portable zip and deploy it per-user
(%LOCALAPPDATA%\Programs\DevTools + a Start Menu shortcut).

Usage:
    packaging\windows\deploy-desktop.ps1                  # build 0.0.0-dev, then deploy
    packaging\windows\deploy-desktop.ps1 1.2.3
    packaging\windows\deploy-desktop.ps1 -Version 1.2.3
    packaging\windows\deploy-desktop.ps1 -Remove
    packaging\windows\deploy-desktop.ps1 -Help
"@
}

if ($Help) { Show-Help; exit 0 }

# Stable per-user install dir (same convention as VS Code / Discord), so the
# shortcut's target survives version bumps.
$InstallDir  = Join-Path $env:LOCALAPPDATA "Programs\DevTools"
# Shortcut name mirrors the installer's {autoprograms}\Dev Tools entry.
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Dev Tools.lnk"

if ($Remove) {
    if (Test-Path $InstallDir) { Remove-Item -Recurse -Force $InstallDir; Write-Host ":: removed $InstallDir" }
    if (Test-Path $ShortcutPath) { Remove-Item -Force $ShortcutPath; Write-Host ":: removed $ShortcutPath" }
    exit 0
}

# Build first; the build script owns version validation, publish, staging,
# verification and the zip. Run as a child process: its `exit` statements
# would otherwise terminate THIS script mid-flow.
$buildScript = Join-Path $PSScriptRoot "build-portable.ps1"
$hostExe = (Get-Process -Id $PID).Path
if (-not $hostExe) { Fail "cannot resolve the current PowerShell executable to invoke the build script." }
& $hostExe -NoProfile -File $buildScript -Version $Version
if ($LASTEXITCODE -ne 0) { Fail "build-portable.ps1 failed (exit $LASTEXITCODE)." }

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Zip = Join-Path $RepoRoot "packaging\windows\portable\DevTools-Portable-$Version.zip"
if (-not (Test-Path $Zip)) { Fail "portable zip not found: $Zip" }

# Wipe-then-extract: unzipping over a previous deploy can leave stale files.
# Safe because user data lives in %USERPROFILE%\.devtools, never here.
if (Test-Path $InstallDir) { Remove-Item -Recurse -Force $InstallDir }
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Expand-Archive -Path $Zip -DestinationPath $InstallDir -Force

# Same content check as the build script — mtimes prove nothing.
foreach ($exe in @("DevTools.exe", "bin\Tools.exe")) {
    $exePath = Join-Path $InstallDir $exe
    if (-not (Test-Path $exePath)) {
        Fail "deployed payload check failed: $exe missing"
    }
    $len = (Get-Item $exePath).Length
    if ($len -lt 40MB) {
        Fail "deployed payload check failed: $exe is only $len bytes (stale or thin payload?)"
    }
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($ShortcutPath)
$shortcut.TargetPath = (Join-Path $InstallDir "DevTools.exe")
$shortcut.WorkingDirectory = $InstallDir
$shortcut.Save()

Write-Host ""
Write-Host ":: DevTools deployed ::"
Write-Host "  $InstallDir"
Write-Host "  $ShortcutPath"
exit 0
