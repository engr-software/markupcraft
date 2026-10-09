; MarkupCraft installer (NSIS 3). Built by packaging/windows/package.ps1, which passes:
;   VERSION           display version, e.g. 0.3.0 or 0.4.0-rc.1
;   NUMERIC_VERSION   X.Y.Z.W for the file version resource
;   STAGE             folder holding markupcraft.exe, markupcraft-cli.exe, docs and the icon
;   ICON              the app icon (.ico)
;   OUTFILE           the setup .exe to write
;
; Per-machine install into Program Files, a Start menu entry, an "Open with" entry for PDFs,
; and an uninstaller listed under Apps & features. The uninstaller removes only the files it
; installed.

Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!define APP "MarkupCraft"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"
!define APPS_KEY "Software\Classes\Applications\markupcraft.exe"

Name "${APP} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APP}"
InstallDirRegKey HKLM "Software\${APP}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
ManifestDPIAware true

VIProductVersion "${NUMERIC_VERSION}"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "LegalCopyright" "The MarkupCraft authors. MIT OR Apache-2.0."

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\markupcraft.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start ${APP}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "${APP} needs 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"
  File "${STAGE}\markupcraft.exe"
  File "${STAGE}\markupcraft-cli.exe"
  File "${STAGE}\markupcraft.ico"
  File /nonfatal "${STAGE}\README.md"
  File /nonfatal "${STAGE}\LICENSE-MIT"
  File /nonfatal "${STAGE}\LICENSE-APACHE"
  File /nonfatal "${STAGE}\THIRD_PARTY.md"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\markupcraft.exe" "" "$INSTDIR\markupcraft.ico"

  ; "Open with" for PDFs, without taking over the default PDF viewer.
  WriteRegStr HKLM "${APPS_KEY}" "FriendlyAppName" "${APP}"
  WriteRegStr HKLM "${APPS_KEY}\DefaultIcon" "" "$INSTDIR\markupcraft.ico"
  WriteRegStr HKLM "${APPS_KEY}\shell\open\command" "" '"$INSTDIR\markupcraft.exe" "%1"'
  WriteRegStr HKLM "${APPS_KEY}\SupportedTypes" ".pdf" ""
  WriteRegStr HKLM "Software\Classes\.pdf\OpenWithList\markupcraft.exe" "" ""

  WriteRegStr HKLM "Software\${APP}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "The MarkupCraft authors"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\markupcraft.ico"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\${APP}.lnk"
  Delete "$INSTDIR\markupcraft.exe"
  Delete "$INSTDIR\markupcraft-cli.exe"
  Delete "$INSTDIR\markupcraft.ico"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\THIRD_PARTY.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${APPS_KEY}"
  DeleteRegKey HKLM "Software\Classes\.pdf\OpenWithList\markupcraft.exe"
  DeleteRegKey HKLM "Software\${APP}"
  DeleteRegKey HKLM "${UNINST_KEY}"
SectionEnd
