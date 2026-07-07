#define MyAppName "Dev Tools"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "DevTools"
#define MyAppExeName "DevTools.exe"

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName=C:\{#MyAppPublisher}
DisableProgramGroupPage=yes
OutputDir=installer
OutputBaseFilename=DevTools-Setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=src\Tools\Assets\logo.ico
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Everything except user-data JSONs, which are handled below so reinstalls preserve them.
Source: "bin\output\win-x64\publish\*"; DestDir: "{app}"; Excludes: "settings\settings.json,settings\repos.cache.json,settings\opencode\models.json,settings\snapit\Settings.json,settings\snapit\ExcludedApplicationSettings.json,settings\snapit\layouts\*.json"; Flags: ignoreversion recursesubdirs createallsubdirs
; User-data files: ship defaults on fresh install, keep existing on upgrade so the app
; can migrate them to %USERPROFILE%\.devtools on first run. neveruninstall so a
; repair/upgrade never drops the migration source.
Source: "bin\output\win-x64\publish\settings\settings.json"; DestDir: "{app}\settings"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "bin\output\win-x64\publish\settings\opencode\models.json"; DestDir: "{app}\settings\opencode"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "bin\output\win-x64\publish\settings\snapit\Settings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "bin\output\win-x64\publish\settings\snapit\ExcludedApplicationSettings.json"; DestDir: "{app}\settings\snapit"; Flags: onlyifdoesntexist uninsneveruninstall
Source: "bin\output\win-x64\publish\settings\snapit\layouts\*.json"; DestDir: "{app}\settings\snapit\layouts"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[Code]
procedure LaunchApplication();
var
  ResultCode: Integer;
begin
  if MsgBox('Do you want to launch Dev Tools now?', mbConfirmation, MB_YESNO) = IDYES then
  begin
    Exec(ExpandConstant('{app}\{#MyAppExeName}'), '', '', SW_SHOW, ewNoWait, ResultCode);
  end;
end;
