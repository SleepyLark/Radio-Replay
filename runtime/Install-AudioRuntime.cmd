@echo off
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0runtime\Install-AudioRuntime.ps1"
if errorlevel 1 (
  echo Audio runtime setup failed. Check your connection and run this file again.
  pause
  exit /b 1
)
