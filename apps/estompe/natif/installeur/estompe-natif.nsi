Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "Sections.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE
  !define EXE "..\target\x86_64-pc-windows-msvc\release\estompe-natif.exe"
!endif
!ifndef FFMPEG
  !define FFMPEG "..\..\windows\src-tauri\binaries\ffmpeg.exe"
!endif
!ifndef MPV
  !define MPV "..\target\mpv\libmpv-2.dll"
!endif
!define NOM "Estompe natif (bêta)"
!define EXECUTABLE "estompe-natif.exe"
!define ICONE "..\icones\estompe.ico"
!define RACCOURCI "Estompe natif (bêta).lnk"
!define CLE_APP "Software\EstompeNatif"
!define CLE_DESINSTALLATION "Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif"
!define CLE_AUMID "Software\Classes\AppUserModelId\fr.plbls.estompe.natif"
!define CLE_DEMARRAGE "Software\Microsoft\Windows\CurrentVersion\Run"
!define VALEUR_DEMARRAGE "EstompeNatif"
!define CACHE "$LOCALAPPDATA\fr.plbls.estompe.natif"

Name "${NOM}"
OutFile "Estompe-natif-beta-${VERSION}-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Estompe natif"
InstallDirRegKey HKCU "${CLE_APP}" "Dossier"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${NOM}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "Installeur d'${NOM}"
VIAddVersionKey "LegalCopyright" "plbls.fr"

Var Relancer

!define MUI_ICON "${ICONE}"
!define MUI_UNICON "${ICONE}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXECUTABLE}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "French"

!macro ATTENDRE_FERMETURE PREFIXE
  Function ${PREFIXE}AttendreFermeture
    StrCpy $0 0
    essai:
      IfFileExists "$INSTDIR\${EXECUTABLE}" 0 libre
      ClearErrors
      FileOpen $1 "$INSTDIR\${EXECUTABLE}" a
      IfErrors occupe
      FileClose $1
      Goto libre
    occupe:
      ${If} ${Silent}
        IntOp $0 $0 + 1
        ${If} $0 < 120
          Sleep 500
          Goto essai
        ${EndIf}
        Abort
      ${EndIf}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Fermez Estompe natif (icône de la zone de notification, « Quitter »), puis réessayez." IDRETRY essai
      Abort
    libre:
  FunctionEnd
!macroend
!insertmacro ATTENDRE_FERMETURE ""
!insertmacro ATTENDRE_FERMETURE "un."

Section "Estompe natif" SecProgramme
  SectionIn RO
  SetOutPath "$INSTDIR"
  Call AttendreFermeture
  File "/oname=${EXECUTABLE}" "${EXE}"
  File "/oname=ffmpeg.exe" "${FFMPEG}"
  File "/oname=libmpv-2.dll" "${MPV}"
  File "/oname=estompe.ico" "${ICONE}"
  WriteUninstaller "$INSTDIR\desinstaller.exe"
  Delete "$SMPROGRAMS\Estompe natif (beta).lnk"
  CreateShortcut "$SMPROGRAMS\${RACCOURCI}" "$INSTDIR\${EXECUTABLE}" "" "$INSTDIR\estompe.ico"
  WriteRegStr HKCU "${CLE_APP}" "Dossier" "$INSTDIR"
  WriteRegStr HKCU "${CLE_AUMID}" "DisplayName" "${NOM}"
  WriteRegStr HKCU "${CLE_AUMID}" "IconUri" "$INSTDIR\estompe.ico"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "DisplayName" "${NOM}"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "DisplayIcon" "$INSTDIR\estompe.ico"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "Publisher" "plbls.fr"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "UninstallString" "$\"$INSTDIR\desinstaller.exe$\""
  WriteRegStr HKCU "${CLE_DESINSTALLATION}" "QuietUninstallString" "$\"$INSTDIR\desinstaller.exe$\" /S"
  WriteRegDWORD HKCU "${CLE_DESINSTALLATION}" "NoModify" 1
  WriteRegDWORD HKCU "${CLE_DESINSTALLATION}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${CLE_DESINSTALLATION}" "EstimatedSize" $0
SectionEnd

Section "Raccourci sur le bureau" SecBureau
  CreateShortcut "$DESKTOP\${RACCOURCI}" "$INSTDIR\${EXECUTABLE}" "" "$INSTDIR\estompe.ico"
SectionEnd

Function .onInit
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/RELANCER" $R1
  ${IfNot} ${Errors}
    StrCpy $Relancer 1
  ${EndIf}
  ${If} ${Silent}
    !insertmacro UnselectSection ${SecBureau}
  ${EndIf}
FunctionEnd

Function .onInstSuccess
  ${If} $Relancer == 1
    Exec '"$INSTDIR\${EXECUTABLE}"'
  ${EndIf}
FunctionEnd

Function .onInstFailed
  ${If} $Relancer == 1
  ${AndIf} ${FileExists} "$INSTDIR\${EXECUTABLE}"
    Exec '"$INSTDIR\${EXECUTABLE}"'
  ${EndIf}
FunctionEnd

Section "Uninstall"
  Call un.AttendreFermeture
  Delete "$INSTDIR\${EXECUTABLE}"
  Delete "$INSTDIR\ffmpeg.exe"
  Delete "$INSTDIR\libmpv-2.dll"
  Delete "$INSTDIR\estompe.ico"
  Delete "$INSTDIR\desinstaller.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${RACCOURCI}"
  Delete "$DESKTOP\${RACCOURCI}"
  DeleteRegKey HKCU "${CLE_APP}"
  DeleteRegKey HKCU "${CLE_DESINSTALLATION}"
  DeleteRegKey HKCU "${CLE_AUMID}"
  DeleteRegValue HKCU "${CLE_DEMARRAGE}" "${VALEUR_DEMARRAGE}"
  RMDir /r "${CACHE}\cache"
  RMDir "${CACHE}"
SectionEnd
