; Inno Setup 6 script — per-user installer for DevTools.
; build.ps1 -Installer invokes this with /DMyAppVersion=<cargo version>;
; the fallback below keeps plain `ISCC devtools.iss` working too.

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
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; SnapIt companion (Windows-only): exe bundles the managed code, native libs stay
; beside it. Seed defaults install only when missing so user edits survive upgrades.
Source: "out\snapit-publish\Tools.SnapIt.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "out\snapit-publish\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "out\snapit-publish\settings\snapit\Settings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "out\snapit-publish\settings\snapit\ExcludedApplicationSettings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "out\snapit-publish\settings\snapit\layouts\*.json"; DestDir: "{app}\settings\snapit\layouts"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch DevTools"; \
    Flags: nowait postinstall skipifsilent
