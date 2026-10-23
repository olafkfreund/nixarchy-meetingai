; Build with scripts/build-windows-installer.ps1 (Inno Setup 6.3+).
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef SourceBinary
  #error SourceBinary is required
#endif
#ifndef OutputDirectory
  #error OutputDirectory is required
#endif

[Setup]
AppId={{72AF4F45-C286-4FB8-BB11-AD2C8C83985C}
AppName=Lennette
AppVersion={#AppVersion}
AppPublisher=Lennette contributors
AppPublisherURL=https://github.com/parazeeknova/lennette
DefaultDirName={localappdata}\Programs\Lennette
DefaultGroupName=Lennette
DisableProgramGroupPage=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir={#OutputDirectory}
OutputBaseFilename=lennette-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\lennette.exe
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceBinary}"; DestDir: "{app}"; DestName: "lennette.exe"; Flags: ignoreversion
Source: "..\..\.github\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\docs\windows.md"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{group}\Lennette"; Filename: "{app}\lennette.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Lennette"; Filename: "{app}\lennette.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\lennette.exe"; Description: "Launch Lennette"; Flags: nowait postinstall skipifsilent

; No AppData deletion: uninstall keeps settings, history and the credential vault.
