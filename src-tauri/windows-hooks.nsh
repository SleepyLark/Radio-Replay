!macro NSIS_HOOK_POSTINSTALL
  ; Remove obsolete setup helpers when upgrading a download-on-install build.
  ; The runtime DLL is now an ordinary bundled resource managed by the installer.
  Delete "$INSTDIR\Install-AudioRuntime.cmd"
  Delete "$INSTDIR\runtime\Install-AudioRuntime.ps1"
!macroend
