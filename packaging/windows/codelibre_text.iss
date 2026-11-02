; Windows installer, built by .github/workflows/release.yml with Inno Setup:
;   ISCC.exe /DAppVersion=0.1.0-alpha packaging\windows\codelibre_text.iss
; Run it after `cargo build --release`. The setup goes to dist\ in the repository root.

#ifndef AppVersion
  #error Pass the version: ISCC.exe /DAppVersion=0.1.0-alpha
#endif

[Setup]
; Keep this id: it is how Windows finds the installed version to upgrade.
AppId={{6F4E2B7A-3C1D-4A58-9E0B-7D2C5A8F1E36}
AppName=CodeLibre Text
AppVersion={#AppVersion}
AppPublisher=Oscar Chacon Cautin
AppPublisherURL=https://github.com/oscarchacon/codelibre_text
DefaultDirName={autopf}\CodeLibre Text
DefaultGroupName=CodeLibre Text
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\..\assets\icons\codelibre_text.ico
UninstallDisplayIcon={app}\codelibre_text.exe
OutputDir=..\..\dist
OutputBaseFilename=codelibre-text-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\codelibre_text.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\CodeLibre Text"; Filename: "{app}\codelibre_text.exe"
Name: "{autodesktop}\CodeLibre Text"; Filename: "{app}\codelibre_text.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\codelibre_text.exe"; Description: "{cm:LaunchProgram,CodeLibre Text}"; Flags: nowait postinstall skipifsilent
