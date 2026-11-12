!macro NSIS_HOOK_POSTINSTALL
  Push $R0
  nsExec::ExecToLog /TIMEOUT=240000 '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\runtime\Install-AudioRuntime.ps1" -Destination "$INSTDIR\audio"'
  Pop $R0
  ${If} $R0 != 0
    MessageBox MB_ICONSTOP|MB_OK "The audio runtime could not be installed. Check your internet connection and run Install-AudioRuntime.cmd from the application folder, or rerun this installer." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  Pop $R0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\audio\libmpv-2.dll"
  Delete "$INSTDIR\audio\LICENSE.LGPL"
  RMDir "$INSTDIR\audio"
!macroend
