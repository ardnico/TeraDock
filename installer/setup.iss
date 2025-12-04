; Placeholder Inno Setup script for TeraDock ttlaunch installer
[Setup]
AppName=TeraDock ttlaunch
AppVersion=0.1.0
DefaultDirName={pf}\\TeraDock\\ttlaunch
DefaultGroupName=TeraDock
OutputDir=dist
OutputBaseFilename=setup

[Files]
Source: "..\\target\\release\\cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\\target\\release\\gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\\config\\default_profiles.toml"; DestDir: "{app}\\config"; Flags: ignoreversion

[Icons]
Name: "{group}\\TeraDock ttlaunch GUI"; Filename: "{app}\\gui.exe"
Name: "{group}\\TeraDock ttlaunch CLI"; Filename: "{app}\\cli.exe"
