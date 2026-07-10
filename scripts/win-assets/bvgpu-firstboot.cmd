@echo off
rem BridgeVM first-boot GPU driver activation, THREE-STAGE. Runs elevated via an
rem HKLM RunOnce planted by bvinject.cmd; each stage re-arms the RunOnce and
rem reboots, so the next stage runs on the following boot.
rem
rem WHY THREE STAGES: if pnputil /install runs while BCD testsigning is still OFF
rem (i.e. in the same pass that first enables it), Windows records the device as
rem CM_PROB_NEED_RESTART (14) because it cannot start the test-signed driver yet,
rem and that pre-testsigning install state can persist as a failed start
rem (CM_PROB_FAILED_POST_START / Code 43) even after the reboot. So:
rem   Stage 1: enable testsigning + trust the cert, then reboot.
rem   Stage 2: pnputil /install with testsigning ALREADY ACTIVE, then reboot.
rem   Stage 3: verify the device state (no reboot).
rem
rem NOTE: a space precedes every redirection operator on purpose (%TIME% ends in a
rem digit, and `<digit>>file` parses as a numbered-stream redirect). Log is
rem APPENDED (>>) so every stage is visible.
setlocal DisableDelayedExpansion
set PKG=C:\BridgeVM\viogpu3d
set CER=%PKG%\BridgeVM-viogpu3d-Test.cer
set LOG=C:\BridgeVM\viogpu3d-firstboot.log
set RO=HKLM\Software\Microsoft\Windows\CurrentVersion\RunOnce
set NEXT_STAGE_2=!BridgeVMGpu3DStage2
set NEXT_STAGE_3=!BridgeVMGpu3DStage3
set STAGE=dispatch

echo [bvgpu-firstboot] invoked %DATE% %TIME% >> "%LOG%"

if not exist C:\BridgeVM\stage1.flag goto :stage1
if not exist C:\BridgeVM\stage2.flag goto :stage2
goto :stage3

:stage1
set STAGE=stage1
echo [stage1] testsigning on + trust cert >> "%LOG%"
bcdedit /set {current} testsigning on >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
certutil -f -addstore Root "%CER%" >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
certutil -f -addstore TrustedPublisher "%CER%" >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
reg add "%RO%" /v %NEXT_STAGE_2% /t REG_SZ /d "cmd /c C:\BridgeVM\bvgpu-firstboot.cmd" /f >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
echo done > C:\BridgeVM\stage1.flag
if errorlevel 1 goto :fail
echo [stage1] rebooting to activate testsigning >> "%LOG%"
shutdown /r /t 5 /c "BridgeVM viogpu3d stage1"
if errorlevel 1 goto :fail
goto :done

:stage2
set STAGE=stage2
echo [stage2] pnputil install with testsigning active >> "%LOG%"
pnputil /add-driver "%PKG%\viogpu3d.inf" /install >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
pnputil /scan-devices >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
reg add "%RO%" /v %NEXT_STAGE_3% /t REG_SZ /d "cmd /c C:\BridgeVM\bvgpu-firstboot.cmd" /f >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
echo done > C:\BridgeVM\stage2.flag
if errorlevel 1 goto :fail
echo [stage2] rebooting to start the freshly-installed driver >> "%LOG%"
shutdown /r /t 5 /c "BridgeVM viogpu3d stage2"
if errorlevel 1 goto :fail
goto :done

:stage3
set STAGE=stage3
echo [stage3] verify PnP status and bound viogpu3d INF >> "%LOG%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$dev = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue ^| Where-Object { $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_(1050^|10F7)' -and $_.Status -eq 'OK' } ^| Select-Object -First 1; if (-not $dev) { Write-Error 'VirtIO GPU device is not present with Status OK'; exit 1 }; $drv = Get-CimInstance Win32_PnPSignedDriver ^| Where-Object { $_.DeviceID -eq $dev.InstanceId } ^| Select-Object -First 1; if (-not $drv -or $drv.InfName -notlike 'oem*.inf') { Write-Error 'VirtIO GPU is not bound to an OEM driver package'; exit 2 }; $inf = Join-Path $env:windir ('INF\\' + $drv.InfName); if (-not (Test-Path $inf) -or -not (Select-String -Path $inf -Pattern 'viogpu3d' -Quiet)) { Write-Error ('Bound INF is not viogpu3d: ' + $inf); exit 3 }; $dev ^| Format-List Status,Class,FriendlyName,InstanceId; $drv ^| Format-List DeviceName,DriverVersion,DriverProviderName,InfName" >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
echo [stage3] done %DATE% %TIME% >> "%LOG%"
goto :done

:fail
set FAIL_STATUS=%ERRORLEVEL%
if "%FAIL_STATUS%"=="0" set FAIL_STATUS=1
echo [failure] stage=%STAGE% errorlevel=%FAIL_STATUS% %DATE% %TIME% >> "%LOG%"
endlocal & exit /b %FAIL_STATUS%

:done
endlocal
