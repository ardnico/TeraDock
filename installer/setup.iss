; Installer script for the SSH-first TeraDock launcher
[Setup]
AppName=TeraDock SSH Launcher
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

[Code]
function HasOpenSSH(): Boolean;
begin
  Result := FileExists(ExpandConstant('{sys}\\OpenSSH\\ssh.exe')) or FileExists(ExpandConstant('{sys}\\ssh.exe'));
end;

procedure InitializeWizard();
begin
  if not HasOpenSSH() then
  begin
    MsgBox('OpenSSH (ssh.exe) was not found. TeraDock expects either the Windows optional OpenSSH client or a custom ssh.exe on PATH. Proceeding will install TeraDock, but connections will fail until ssh.exe is available.', mbInformation, MB_OK);
  end;
end;
