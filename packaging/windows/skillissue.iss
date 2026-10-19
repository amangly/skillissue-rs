; Values are supplied by cargo installer from Cargo metadata.
#ifndef AppVersion
  #error AppVersion is required
#endif
[Setup]
AppId={{E764810B-A67A-49F4-B3F6-D88E18D024BC}
AppName=skillissue
AppVersion={#AppVersion}
AppPublisher=amangly
AppCopyright=Copyright (c) 2026 amangly
AppPublisherURL=https://github.com/amangly/skillissue-rs
DefaultDirName={localappdata}\Programs\skillissue
DefaultGroupName=skillissue
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.22000
DisableProgramGroupPage=yes
LicenseFile={#SourceRoot}\LICENSE
SetupIconFile={#SourceRoot}\assets\branding\icon.ico
UninstallDisplayIcon={app}\skillissue.ico
ChangesAssociations=yes
OutputDir={#OutputPath}
OutputBaseFilename=skillissue-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=skillissue.exe
RestartApplications=no
UninstallDisplayName=skillissue

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; DestName: "skillissue.exe"; Flags: ignoreversion
Source: "{#SourceRoot}\assets\branding\icon.ico"; DestDir: "{app}"; DestName: "skillissue.ico"; Flags: ignoreversion
Source: "{#RuntimeDir}\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RuntimeDir}\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\assets\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\skillissue"; Filename: "{app}\skillissue.exe"; IconFilename: "{app}\skillissue.ico"
Name: "{userdesktop}\skillissue"; Filename: "{app}\skillissue.exe"; IconFilename: "{app}\skillissue.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "skillissue"; ValueData: """{app}\skillissue.exe"""; Flags: uninsdeletevalue

[UninstallDelete]
; Only installation-local scratch files; never sweep the user's Temp directory.
Type: files; Name: "{app}\*.tmp"
Type: files; Name: "{app}\*.log"
Type: dirifempty; Name: "{app}"

[Run]
Filename: "{app}\skillissue.exe"; Description: "Open skillissue and set up AI providers"; Flags: nowait postinstall skipifsilent

; Deliberately no profile/credential deletion on uninstall.
