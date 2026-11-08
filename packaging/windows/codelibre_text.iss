; Windows installer, built by .github/workflows/release.yml with Inno Setup:
;   ISCC.exe /DAppVersion=0.1.0-alpha packaging\windows\codelibre_text.iss
; Run it after `cargo build --release`. The setup goes to dist\ in the repository root.
; For ARM64: /DAppArch=arm64 /DBinDir=..\..\target\aarch64-pc-windows-msvc\release

#ifndef AppVersion
  #error Pass the version: ISCC.exe /DAppVersion=0.1.0-alpha
#endif
#ifndef AppArch
  #define AppArch "x64"
#endif
#ifndef BinDir
  #define BinDir "..\..\target\release"
#endif
; Python for the plugins, installed with the editor (offline): sh packaging/python/fetch_python.sh <BinDir>\python
#ifndef PythonDir
  #define PythonDir BinDir + "\python"
#endif
#if AppArch == "arm64"
  #define InstallArchitectures "arm64"
#else
  #define InstallArchitectures "x64compatible"
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
ArchitecturesAllowed={#InstallArchitectures}
ArchitecturesInstallIn64BitMode={#InstallArchitectures}
SetupIconFile=..\..\assets\icons\codelibre_text.ico
UninstallDisplayIcon={app}\codelibre_text.exe
OutputDir=..\..\dist
OutputBaseFilename=codelibre-text-{#AppVersion}-windows-{#AppArch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinDir}\codelibre_text.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "{#PythonDir}\*"; DestDir: "{app}\python"; Flags: ignoreversion recursesubdirs createallsubdirs

; An upgrade replaces the whole Python (no module of an older one left behind); the uninstaller
; also takes the bytecode the plugins compiled there.
[InstallDelete]
Type: filesandordirs; Name: "{app}\python"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\python"

[Icons]
Name: "{autoprograms}\CodeLibre Text"; Filename: "{app}\codelibre_text.exe"
Name: "{autodesktop}\CodeLibre Text"; Filename: "{app}\codelibre_text.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\codelibre_text.exe"; Description: "{cm:LaunchProgram,CodeLibre Text}"; Flags: nowait postinstall skipifsilent
