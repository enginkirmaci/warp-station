#Requires -Version 5.1
<#
build-portable.ps1 — generate a self-contained Windows (win-x64) portable build,
mirroring the CI workflow in .github/workflows/build-installer.yml (publish +
stage + zip steps). This is the Windows-native counterpart of build-portable.sh:
it uses robocopy (the rsync/CI equivalent) for staging and Compress-Archive for
zipping, so a Windows dev box can build the same artifact locally.

Usage:
    ./build-portable.ps1                    # version defaults to 0.0.0-dev
    ./build-portable.ps1 1.2.3              # version = 1.2.3
    ./build-portable.ps1 -Version 1.2.3     # version = 1.2.3
    ./build-portable.ps1 -Help              # show this help

Output: portable\DevTools-Portable-<version>.zip
The zip runs on a clean Windows box (no .NET install needed) and stores its
user data in %USERPROFILE%\.devtools, seeded from the bundled settings\ tree.
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
build-portable.ps1 — generate a self-contained Windows (win-x64) portable build.

Usage:
    ./build-portable.ps1                    # version defaults to 0.0.0-dev
    ./build-portable.ps1 1.2.3              # version = 1.2.3
    ./build-portable.ps1 -Version 1.2.3     # version = 1.2.3
    ./build-portable.ps1 -Help              # show this help

Output:
    portable\DevTools-Portable-<version>.zip

The zip runs on a clean Windows box (no .NET install needed) and stores its
user data in %USERPROFILE%\.devtools, seeded from the bundled settings\ tree.
"@
}

# --- Locate repo root (this script lives at the repo root, but be defensive) ---
if ($Help) { Show-Help; exit 0 }
Set-Location $PSScriptRoot

# Validate version shape (same regex as the CI workflow and build-portable.sh).
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

# --- Paths (must match CI: split build output -> portable-stage -> portable\) ---
# DevTools (launcher) and Tools (main app) publish to separate folders per the csproj
# OutputPath layout (DevTools -> build\, Tools -> build\bin\). The portable zip mirrors
# the installer: DevTools at the zip root, Tools under a bin\ subfolder.
$DevToolsPublish = "build\win-x64\publish"
$ToolsPublish    = "build\bin\win-x64\publish"
$StageDir        = "build\portable-stage"
$OutDir          = "portable"
$Zip             = Join-Path $OutDir "DevTools-Portable-$Version.zip"

# Publish flags identical to .github/workflows/build-installer.yml lines 54/57.
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
# robocopy mirrors CI's `robocopy /E /XF *.pdb *.xml` (recursive copy, two excludes):
# DevTools -> stage root; Tools -> stage\bin (split layout, matching the installer).
# Robocopy reports success as exit 0-7; 8+ is an error. Reset LASTEXITCODE afterward
# so it doesn't leak into the next native call or the script's final exit code.
robocopy $DevToolsPublish $StageDir /E /XF *.pdb *.xml | Out-Null
if ($LASTEXITCODE -ge 8) { Fail "robocopy DevTools failed with exit code $LASTEXITCODE." }
$global:LASTEXITCODE = 0

robocopy $ToolsPublish (Join-Path $StageDir "bin") /E /XF *.pdb *.xml | Out-Null
if ($LASTEXITCODE -ge 8) { Fail "robocopy Tools failed with exit code $LASTEXITCODE." }
$global:LASTEXITCODE = 0

Write-Host "=== Create portable zip ==="
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (Test-Path $Zip) { Remove-Item -Force $Zip }
# Compress-Archive with a wildcard path puts the stage *contents* at the zip root
# (DevTools.exe at the root), matching the installer's {app} layout. CompressionLevel
# Optimal corresponds to the Python zipfile compresslevel=6 used by build-portable.sh.
Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $Zip -CompressionLevel Optimal

$zipFull = (Resolve-Path $Zip).Path
$sizeMB  = [math]::Round((Get-Item $Zip).Length / 1MB, 1)

Write-Host ""
Write-Host ":: Portable build ready ::"
Write-Host "  $zipFull"
Write-Host "  ($sizeMB MB)"
