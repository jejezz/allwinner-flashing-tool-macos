; Inno Setup script template for a Flutter Windows desktop app
; (jejezz/application-release-templates desktop/ @ conventions-v1).
;
; Run by .github/workflows/release.yml's `build-windows` job on every
; `vX.Y.Z` tag push, which passes MyAppName / MyFileName / MyAppVersion /
; MyAppNumericVersion / MyAppExeName via `ISCC /D...` (read from the tag,
; AppInfo.xcconfig and windows/CMakeLists.txt). The fallbacks below are only
; for a local, manual compile — MyAppVersion 0.0.0 marks such a build.
;
; This repo keeps the Flutter app in gui/, so the paths below reach into
; gui/ instead of the template's project-root paths. The Rust helper
; (aw-tool.exe) is copied into the Release folder before ISCC runs, so it
; ships with the rest of the bundle.
;
; AppId is the GUID every Allwinner Flasher installer since v0.1.0 has used.
; Never change it, or Windows treats the next version as a different
; application (breaking upgrade/uninstall). conventions/identity.md §5.

#ifndef MyAppName
  #define MyAppName "Allwinner Flasher"
#endif
#ifndef MyFileName
  #define MyFileName "AllwinnerFlasher"
#endif
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef MyAppNumericVersion
  #define MyAppNumericVersion "0.0.0"
#endif
#ifndef MyAppExeName
  #define MyAppExeName "aw_flasher.exe"
#endif
#define MyAppPublisher "Jongyun Ahn"
#define MyAppURL "https://github.com/jejezz/allwinner-flashing-tool-macos"
; The app's first release year — edit it for apps started after 2026.
#define MyFirstReleaseYear "2026"
#define SourceDir "..\..\gui\build\windows\x64\runner\Release"

[Setup]
AppId={{B7B6E1A0-6E6D-4C3E-9C1A-1F3AEFE80001}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
AppCopyright=Copyright (C) {#MyFirstReleaseYear} {#MyAppPublisher}
VersionInfoVersion={#MyAppNumericVersion}
VersionInfoProductName={#MyAppName}
VersionInfoCompany={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename={#MyFileName}-{#MyAppVersion}-windows-x64-setup
SetupIconFile=..\..\gui\windows\runner\resources\app_icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; No license page: MIT apps have nothing to agree to (conventions/licensing.md §4).

[Languages]
; Installer text comes from Inno Setup's own translations — don't hardcode
; Korean (or English) strings below (conventions/packaging.md §3).
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[InstallDelete]
; Installers up to v1.1.3 put the Start menu shortcut in its own folder
; ({group} = Programs\Allwinner Flasher\); it now sits directly under
; Programs. Remove the old folder so upgraded machines don't show two.
Type: filesandordirs; Name: "{group}"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
