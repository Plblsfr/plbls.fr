Unicode true
RequestExecutionLevel user
!include "MUI2.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE
  !define EXE "..\target\x86_64-pc-windows-msvc\release\estompe-natif.exe"
!endif

Name "Estompe natif (beta)"
OutFile "Estompe-natif-beta-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Estompe natif"
InstallDirRegKey HKCU "Software\EstompeNatif" "Dossier"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Estompe natif (beta)"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "Installeur d'Estompe natif (beta)"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "French"

Section "Installer"
  SetOutPath "$INSTDIR"
  File "/oname=estompe-natif.exe" "${EXE}"
  WriteUninstaller "$INSTDIR\desinstaller.exe"
  CreateShortcut "$SMPROGRAMS\Estompe natif (beta).lnk" "$INSTDIR\estompe-natif.exe"
  WriteRegStr HKCU "Software\EstompeNatif" "Dossier" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif" "DisplayName" "Estompe natif (beta)"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif" "UninstallString" "$\"$INSTDIR\desinstaller.exe$\""
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif" "NoModify" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\estompe-natif.exe"
  Delete "$INSTDIR\desinstaller.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Estompe natif (beta).lnk"
  DeleteRegKey HKCU "Software\EstompeNatif"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif"
SectionEnd
