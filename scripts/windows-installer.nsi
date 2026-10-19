; Current-user graphical installer. No administrator rights or PATH edits.
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef PAYLOAD_DIR
  !error "PAYLOAD_DIR is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
Name "Tank Yard"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\Tank Yard"
InstallDirRegKey HKCU "Software\TankYard" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetDatablockOptimize on
BrandingText "Tank Yard ${VERSION}"
ShowInstDetails show
ShowUninstDetails show
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Tank Yard"
VIAddVersionKey "FileDescription" "Tank Yard Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Tank Yard contributors"
!define MUI_ICON "${PAYLOAD_DIR}\tankyard.ico"
!define MUI_UNICON "${PAYLOAD_DIR}\tankyard.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Welcome to Tank Yard"
!define MUI_WELCOMEPAGE_TEXT "Install Tank Yard for your Windows account.$\r$\n$\r$\nPlay offline immediately, or join an online arena. Your settings and progress are kept when updating or uninstalling."
!define MUI_FINISHPAGE_RUN "$INSTDIR\tankyard.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Play Tank Yard"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PAYLOAD_DIR}\LICENSE-MIT"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Tank Yard requires 64-bit Windows."
    Abort
  ${EndIf}
  SetShellVarContext current
FunctionEnd

Section "Tank Yard (required)" Game
  SectionIn RO
  SetOutPath "$INSTDIR"
  ClearErrors
  File /r /x configs "${PAYLOAD_DIR}\*"
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONSTOP "Setup could not copy the game. Close Tank Yard and try again."
    Abort
  ${EndIf}
  ; Keep an existing host configuration when updating the game.
  SetOutPath "$INSTDIR\configs"
  SetOverwrite off
  File "${PAYLOAD_DIR}\configs\*.toml"
  SetOverwrite on
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\TankYard" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "DisplayName" "Tank Yard"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "Publisher" "Tank Yard contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "DisplayIcon" "$INSTDIR\tankyard.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard" "NoRepair" 1
  CreateDirectory "$SMPROGRAMS\Tank Yard"
  CreateShortcut "$SMPROGRAMS\Tank Yard\Tank Yard.lnk" "$INSTDIR\tankyard.exe" "" "$INSTDIR\tankyard.ico"
  CreateShortcut "$SMPROGRAMS\Tank Yard\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Desktop shortcut" Desktop
  CreateShortcut "$DESKTOP\Tank Yard.lnk" "$INSTDIR\tankyard.exe" "" "$INSTDIR\tankyard.ico"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  ; Generated exact file list: never recursively erase a user-selected folder.
  !include "${UNINSTALL_FILES}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  ReadRegStr $0 HKCU "Software\TankYard" "InstallDir"
  ${If} $0 == $INSTDIR
    Delete "$SMPROGRAMS\Tank Yard\Tank Yard.lnk"
    Delete "$SMPROGRAMS\Tank Yard\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Tank Yard"
    Delete "$DESKTOP\Tank Yard.lnk"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\TankYard"
    DeleteRegKey HKCU "Software\TankYard"
  ${EndIf}
SectionEnd
