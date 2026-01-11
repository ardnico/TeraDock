#define AppName "TeraDock"
#define AppExeName "td.exe"
#define AppPublisher "TeraDock"
#define AppId "TeraDock"

#ifndef MyAppVersion
#define MyAppVersion "0.1.0"
#endif

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#MyAppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
OutputDir={#SourcePath}\..\..\dist
OutputBaseFilename=td-{#MyAppVersion}-windows-x86_64-setup
Compression=lzma
SolidCompression=yes
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

[Files]
Source: "{#SourcePath}\..\..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
Name: "{userappdata}\{#AppName}"; Flags: uninsalwaysuninstall

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\{#AppName}"
