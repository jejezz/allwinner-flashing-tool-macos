; Inno Setup script for Allwinner Flasher.
;
; Not run automatically by anything in scripts/ — see docs/windows-installer.md
; for the manual steps (build the release folder first, bump MyAppVersion
; below to match pubspec.yaml, then compile this file with ISCC.exe).

#define MyAppName "Allwinner Flasher"
#define MyAppVersion "0.1.0"
#define MyAppExeName "aw_flasher.exe"
#define SourceDir "..\..\gui\build\windows\x64\runner\Release"

[Setup]
; Fixed once and never changed — this is how Windows recognises upgrades of
; the same app across versions. Generate a new one only for a genuinely
; different application, e.g. via PowerShell: [guid]::NewGuid()
AppId={{B7B6E1A0-6E6D-4C3E-9C1A-1F3AEFE80001}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=jyahn
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
OutputDir=..\..\dist
OutputBaseFilename=AllwinnerFlasherSetup-{#MyAppVersion}
SetupIconFile=..\..\gui\windows\runner\resources\app_icon.ico
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExeName}

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "바탕화면에 바로가기 만들기"; GroupDescription: "추가 아이콘:"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "설치 후 바로 실행"; Flags: nowait postinstall skipifsilent
