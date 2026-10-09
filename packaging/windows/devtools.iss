; Inno Setup script for the Rust DevTools app (per-user install).
; Build after build-portable.ps1: ISCC.exe packaging\windows\devtools.iss

#define AppName "DevTools"
#define AppVersion GetVersionNumbersString("..\..\target\release\devtools.exe")
#define AppPublisher "Engin Kirmaci"

[Setup]
AppId={{8E4C2A70-9B31-4E2B-9C7A-RUSTDEVTOOLS01}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\DevTools
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputBaseFilename=DevTools-{#AppVersion}-setup
OutputDir=portable
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\..\crates\devtools\packaging\windows\devtools.ico

[Files]
Source: "..\..\target\release\devtools.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\DevTools"; Filename: "{app}\devtools.exe"
Name: "{autodesktop}\DevTools"; Filename: "{app}\devtools.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Run]
Filename: "{app}\devtools.exe"; Description: "Launch DevTools"; Flags: nowait postinstall skipifsilent
