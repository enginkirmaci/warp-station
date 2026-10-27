; Inno Setup 6 script - optional per-user installer for DevTools, compiled
; manually with ISCC after build.ps1 has staged out\. The version fallback
; keeps plain `ISCC devtools.iss` working too.

#ifndef MyAppVersion
#define MyAppVersion "0.0.0-dev"
#endif

#define MyAppName "DevTools"
#define MyAppExeName "devtools.exe"

[Setup]
AppId={{8E4C2A70-9B31-4E2B-9C7A-RUSTDEVTOOLS01}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Engin Kirmaci
DefaultDirName={localappdata}\Programs\DevTools
DefaultGroupName={#MyAppName}
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=out
OutputBaseFilename=DevTools-{#MyAppVersion}-setup-x64
SetupIconFile=devtools.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "out\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; SnapIt companion (Windows-only): self-contained single-file exe, no loose
; dlls. Seed defaults install only when missing so user edits survive upgrades.
Source: "out\Tools.SnapIt.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "out\settings\snapit\Settings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "out\settings\snapit\ExcludedApplicationSettings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "out\settings\snapit\layouts\*.json"; DestDir: "{app}\settings\snapit\layouts"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch DevTools"; \
    Flags: nowait postinstall skipifsilent
