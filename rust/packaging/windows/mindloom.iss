; Mindloom installer for Windows (Inno Setup 6). Build: iscc /DAppVersion=2.0.0 rust\packaging\windows\mindloom.iss
; Installs per user by default (no admin prompt); can install for all users from the dialog.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C3D7F2A-8E41-4B6A-9D25-4F0B1E7A93C1}
AppName=Mindloom
AppVersion={#AppVersion}
AppVerName=Mindloom {#AppVersion}
AppPublisher=Mindloom contributors
AppPublisherURL=https://github.com/imranseoxpert/mindloom
AppSupportURL=https://github.com/imranseoxpert/mindloom/issues
DefaultDirName={autopf}\Mindloom
DefaultGroupName=Mindloom
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=Mindloom-{#AppVersion}-win-x64
SetupIconFile=mindloom.ico
UninstallDisplayIcon={app}\Mindloom.exe
UninstallDisplayName=Mindloom
LicenseFile=..\..\..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\mindloom.exe"; DestDir: "{app}"; DestName: "Mindloom.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Mindloom"; Filename: "{app}\Mindloom.exe"
Name: "{autodesktop}\Mindloom"; Filename: "{app}\Mindloom.exe"; Tasks: desktopicon

[Registry]
; .mindloom files open in Mindloom; XMind, FreeMind and OPML files list it under "Open with"
Root: HKA; Subkey: "Software\Classes\.mindloom"; ValueType: string; ValueName: ""; ValueData: "Mindloom.Map"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mindloom\OpenWithProgids"; ValueType: string; ValueName: "Mindloom.Map"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Mindloom.Map"; ValueType: string; ValueName: ""; ValueData: "Mindloom map"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Mindloom.Map\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\Mindloom.exe,0"
Root: HKA; Subkey: "Software\Classes\Mindloom.Map\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\Mindloom.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xmind\OpenWithProgids"; ValueType: string; ValueName: "Mindloom.Map"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mm\OpenWithProgids"; ValueType: string; ValueName: "Mindloom.Map"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.opml\OpenWithProgids"; ValueType: string; ValueName: "Mindloom.Map"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\Mindloom.exe\SupportedTypes"; ValueType: string; ValueName: ".mindloom"; ValueData: ""; Flags: uninsdeletekey

[Run]
Filename: "{app}\Mindloom.exe"; Description: "{cm:LaunchProgram,Mindloom}"; Flags: nowait postinstall skipifsilent
