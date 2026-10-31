; StageSync - Windows installer.
;
; Built with NSIS 3 (Unicode). The modern MUI2 interface is used; the
; installer is per-user, so it never asks for administrator rights.
;
; The build script passes these values on the command line:
;
;   /DSTAGESYNC_VERSION=0.1.0            the version shown to the user
;   /DSTAGESYNC_VERSION_NUM=0.1.0.0      four numbers for the file properties
;   /DSTAGESYNC_INSTALL_DIR=<path>       where the application is installed
;   /DSTAGESYNC_EXE=<path>               the built stagesync-desktop.exe
;   /DSTAGESYNC_PORTABLE_ZIP=<path>      the portable ZIP, embedded as an extra
;   /DSTAGESYNC_NOTICES_DIR=<path>       the licence and notice bundle
;
; Every one of them has a default, so the script also compiles when it is run
; by hand without any -D switch.
;
; Paths are written with forward slashes on purpose. NSIS resolves a
; backslash inside a string as the start of an escape sequence, so
; "$INSTDIR\notices" is "$INSTDIR" followed by a newline followed by
; "otices", not a directory called "notices". Forward slashes work on
; Windows and cannot be misread.
;
; This file is UTF-8 with a byte order mark: that is what NSIS needs to read
; the non-ASCII characters in the description below as text.

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

; --- The values that arrive from the build script -------------------------------

!ifndef STAGESYNC_VERSION
  !define STAGESYNC_VERSION "0.0.0"
!endif
!ifndef STAGESYNC_VERSION_NUM
  !define STAGESYNC_VERSION_NUM "0.0.0.0"
!endif
!ifndef STAGESYNC_INSTALL_DIR
  !define STAGESYNC_INSTALL_DIR "$LOCALAPPDATA/Programs/StageSync"
!endif
!ifndef STAGESYNC_EXE
  !define STAGESYNC_EXE "../../target/release/stagesync-desktop.exe"
!endif
!ifndef STAGESYNC_PORTABLE_ZIP
  !define STAGESYNC_PORTABLE_ZIP "../../dist/windows/stagesync-portable.zip"
!endif
!ifndef STAGESYNC_NOTICES_DIR
  !define STAGESYNC_NOTICES_DIR "../../dist/windows/portable/notices"
!endif

!define STAGESYNC_APP_NAME "StageSync"
!define STAGESYNC_PUBLISHER "StageSync contributors"
!define STAGESYNC_EXE_NAME "stagesync-desktop.exe"
!define STAGESYNC_UNINSTALLER_NAME "Uninstall StageSync.exe"
!define STAGESYNC_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\StageSync"
!define STAGESYNC_REG_KEY "Software\StageSync"

!define STAGESYNC_DESCRIPTION "Drift-aware multi-recorder audio synchronisation — StageSync aligns several recordings of the same concert or play against one reference recording, measures the clock drift of each recorder, and renders drift-corrected audio."

Name "${STAGESYNC_APP_NAME} ${STAGESYNC_VERSION}"
OutFile "stagesync-${STAGESYNC_VERSION}-windows-x64-setup.exe"

; A per-user installation: no administrator rights are requested.
RequestExecutionLevel user
SetShellVarContext current

; The define has to be written ${...}. A bare $STAGESYNC_INSTALL_DIR is read
; as the *variable* STAGESYNC_INSTALL_DIR, which is not defined, and the
; installation directory ends up empty.
InstallDir "${STAGESYNC_INSTALL_DIR}"
InstallDirRegKey HKCU "${STAGESYNC_REG_KEY}" "InstallDir"

SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show
BrandingText "${STAGESYNC_APP_NAME} ${STAGESYNC_VERSION}"

; --- The file properties --------------------------------------------------------

VIProductVersion "${STAGESYNC_VERSION_NUM}"
VIAddVersionKey /LANG=1033 "ProductName" "${STAGESYNC_APP_NAME}"
VIAddVersionKey /LANG=1033 "CompanyName" "${STAGESYNC_PUBLISHER}"
VIAddVersionKey /LANG=1033 "FileDescription" "${STAGESYNC_APP_NAME} desktop application"
VIAddVersionKey /LANG=1033 "FileVersion" "${STAGESYNC_VERSION_NUM}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${STAGESYNC_VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "StageSync contributors"
VIAddVersionKey /LANG=1033 "Comments" "${STAGESYNC_DESCRIPTION}"

; --- The pages ------------------------------------------------------------------

!define MUI_ABORTWARNING
!define MUI_WELCOME_TITLE "Welcome to the ${STAGESYNC_APP_NAME} installer"
!define MUI_WELCOME_TEXT "This installs ${STAGESYNC_APP_NAME} version ${STAGESYNC_VERSION} on this computer.$r$n$r$nPublisher: ${STAGESYNC_PUBLISHER}.$r$n$r$n${STAGESYNC_DESCRIPTION}$r$n$r$nThe installation is per-user and needs no administrator rights. Click Next to continue."
!define MUI_FINISHPAGE_TITLE "${STAGESYNC_APP_NAME} ${STAGESYNC_VERSION} is installed"
!define MUI_FINISHPAGE_TEXT "The application is installed in $INSTDIR.$r$n$r$n${STAGESYNC_DESCRIPTION}$r$n$r$nThis build is compiled with the Microsoft Visual C++ toolchain and links the C runtime dynamically, so the machine needs the Microsoft Visual C++ Redistributable. If the application does not start and Windows reports a missing VCRUNTIME140 DLL, install the redistributable from https://aka.ms/vs/17/release/vc_redist.x64.exe.$r$n$r$nThe licence and third-party notice bundle is installed in $INSTDIR/notices."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR/${STAGESYNC_EXE_NAME}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --- The sections ----------------------------------------------------------------

Section "${STAGESYNC_APP_NAME} application" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${STAGESYNC_EXE}"
  WriteUninstaller "$INSTDIR/${STAGESYNC_UNINSTALLER_NAME}"

  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "DisplayName" "${STAGESYNC_APP_NAME} ${STAGESYNC_VERSION}"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "DisplayVersion" "${STAGESYNC_VERSION}"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "Publisher" "${STAGESYNC_PUBLISHER}"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR/${STAGESYNC_EXE_NAME}"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "UninstallString" "$INSTDIR/${STAGESYNC_UNINSTALLER_NAME}"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "QuietUninstallString" "$INSTDIR/${STAGESYNC_UNINSTALLER_NAME} /S"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "NoModify" "1"
  WriteRegStr HKCU "${STAGESYNC_UNINSTALL_KEY}" "NoRepair" "1"
  WriteRegStr HKCU "${STAGESYNC_REG_KEY}" "InstallDir" "$INSTDIR"
SectionEnd

; The licence and third-party notice bundle is part of the distribution, not
; an optional extra: the licences of the linked dependencies require their
; notices to travel with the binary.
Section "Licence and third-party notices" SecNotices
  SetOutPath "$INSTDIR/notices"
  File /r "${STAGESYNC_NOTICES_DIR}/*.*"
SectionEnd

Section "Portable ZIP" SecPortable
  SetOutPath "$INSTDIR/portable"
  File "${STAGESYNC_PORTABLE_ZIP}"
SectionEnd

Section "Start menu shortcut" SecStartMenu
  CreateDirectory "$SMPROGRAMS/${STAGESYNC_APP_NAME}"
  CreateShortcut "$SMPROGRAMS/${STAGESYNC_APP_NAME}/${STAGESYNC_APP_NAME}.lnk" "$INSTDIR/${STAGESYNC_EXE_NAME}"
  CreateShortcut "$SMPROGRAMS/${STAGESYNC_APP_NAME}/Uninstall.lnk" "$INSTDIR/${STAGESYNC_UNINSTALLER_NAME}"
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP/${STAGESYNC_APP_NAME}.lnk" "$INSTDIR/${STAGESYNC_EXE_NAME}"
SectionEnd

; --- The uninstaller -------------------------------------------------------------

Section "Uninstall"
  SetShellVarContext current

  Delete "$INSTDIR/${STAGESYNC_EXE_NAME}"
  Delete "$INSTDIR/${STAGESYNC_UNINSTALLER_NAME}"
  Delete "$INSTDIR/notices/*.*"
  RMDir "$INSTDIR/notices"
  Delete "$INSTDIR/portable/*.zip"
  RMDir "$INSTDIR/portable"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS/${STAGESYNC_APP_NAME}/${STAGESYNC_APP_NAME}.lnk"
  Delete "$SMPROGRAMS/${STAGESYNC_APP_NAME}/Uninstall.lnk"
  RMDir "$SMPROGRAMS/${STAGESYNC_APP_NAME}"
  Delete "$DESKTOP/${STAGESYNC_APP_NAME}.lnk"

  DeleteRegKey HKCU "${STAGESYNC_UNINSTALL_KEY}"
  DeleteRegKey HKCU "${STAGESYNC_REG_KEY}"
SectionEnd
