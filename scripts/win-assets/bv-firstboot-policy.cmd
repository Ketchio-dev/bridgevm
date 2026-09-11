@echo off
setlocal DisableDelayedExpansion
if "%~1"=="" exit /b 2
if "%~2"=="" exit /b 2
if not exist "%~1\" exit /b 2
if not exist "%~2\BridgeVM\" exit /b 2
set "SOURCE=%~1\bridgevm-display-only-firstboot.txt"
set "TARGET=%~2\BridgeVM\bridgevm-display-only-firstboot.txt"
if exist "%SOURCE%\" exit /b 2
if exist "%TARGET%\" exit /b 2
if exist "%SOURCE%" goto :enable
if exist "%TARGET%" del /f /q "%TARGET%" >nul 2>&1
if exist "%TARGET%" (
  echo BVINJECT ERROR: inherited display-only policy could not be removed
  exit /b 1
)
echo BVINJECT FIRSTBOOT_POLICY full
exit /b 0
:enable
copy /y "%SOURCE%" "%TARGET%" >nul
if errorlevel 1 exit /b 1
if not exist "%TARGET%" exit /b 1
echo BVINJECT FIRSTBOOT_POLICY display-only
exit /b 0
