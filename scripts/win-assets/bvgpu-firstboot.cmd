@echo off
rem BridgeVM first-boot GPU driver activation, THREE-STAGE. An elevated HKLM
rem RunOnce planted by bvinject.cmd enters stage 1. Stage 1 then creates one
rem persistent, delayed ONSTART task running as SYSTEM; that task owns stages 2
rem and 3 and deletes itself only after stage 3 succeeds.
rem
rem WHY THREE STAGES: if pnputil /install runs while BCD testsigning is still OFF
rem (i.e. in the same pass that first enables it), Windows records the device as
rem CM_PROB_NEED_RESTART (14) because it cannot start the test-signed driver yet,
rem and that pre-testsigning install state can persist as a failed start
rem (CM_PROB_FAILED_POST_START / Code 43) even after the reboot. So:
rem   Stage 1: enable testsigning + trust the cert, then reboot.
rem   Stage 2: pnputil /install with testsigning ALREADY ACTIVE, then reboot.
rem   Stage 3: verify the device state (no reboot).
rem Each advancing stage records the current Windows boot identity before its
rem reboot. The next stage refuses to run until LastBootUpTime changes, so a
rem canceled reboot, repeated logon, or interrupted RunOnce cannot collapse two
rem stages into one boot.
rem
rem NOTE: a space precedes every redirection operator on purpose (%TIME% ends in a
rem digit, and `<digit>>file` parses as a numbered-stream redirect). Log is
rem APPENDED (>>) so every stage is visible.
setlocal DisableDelayedExpansion
set PKG=C:\BridgeVM\viogpu3d
set CER=%PKG%\BridgeVM-viogpu3d-Test.cer
set LOG=C:\BridgeVM\viogpu3d-firstboot.log
set TASK_NAME=BridgeVM-VioGpu3DFirstBoot
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
echo [stage1] create delayed SYSTEM ONSTART continuation task >> "%LOG%"
schtasks /Create /TN "%TASK_NAME%" /SC ONSTART /DELAY 0001:00 /RU SYSTEM /RL HIGHEST /TR "%ComSpec% /d /c C:\BridgeVM\bvgpu-firstboot.cmd" /F >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
call :write_boot_identity C:\BridgeVM\stage1.boot
if errorlevel 1 goto :fail
echo done > C:\BridgeVM\stage1.flag
if errorlevel 1 goto :fail
echo [stage1] rebooting to activate testsigning >> "%LOG%"
shutdown /r /t 5 /c "BridgeVM viogpu3d stage1"
if errorlevel 1 goto :fail
goto :done

:stage2
set STAGE=stage2
call :require_new_boot C:\BridgeVM\stage1.boot
if errorlevel 1 goto :fail
echo [stage2] pnputil install with testsigning active >> "%LOG%"
pnputil /add-driver "%PKG%\viogpu3d.inf" /install >> "%LOG%" 2>&1
if errorlevel 260 goto :fail
if errorlevel 259 (
  echo [stage2] pnputil reports the package is already current ^(259^); continuing >> "%LOG%"
  goto :stage2_scan
)
if errorlevel 1 goto :fail
:stage2_scan
echo [stage2] release the offline VioGpu3D boot quarantine >> "%LOG%"
sc.exe config VioGpu3D start= demand >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
pnputil /scan-devices >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
echo [stage2] reset persisted display config so the driver's preferred 120Hz mode is selected >> "%LOG%"
reg delete "HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\Configuration" /f >> "%LOG%" 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\Connectivity" /f >> "%LOG%" 2>&1
call :write_boot_identity C:\BridgeVM\stage2.boot
if errorlevel 1 goto :fail
echo done > C:\BridgeVM\stage2.flag
if errorlevel 1 goto :fail
echo [stage2] rebooting to start the freshly-installed driver >> "%LOG%"
shutdown /r /t 5 /c "BridgeVM viogpu3d stage2"
if errorlevel 1 goto :fail
goto :done

:stage3
set STAGE=stage3
call :require_new_boot C:\BridgeVM\stage2.boot
if errorlevel 1 goto :fail
echo [stage3] exercise Vulkan loader and Venus ICD >> "%LOG%"
set VN_DEBUG=init,result
set MESA_LOG_FILE=C:\BridgeVM\bvgpu-mesa-vulkan.log
if exist C:\BridgeVM\bvgpu-vulkan-probe.ps1 powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-vulkan-probe.ps1 >> "%LOG%" 2>&1
set VULKAN_STATUS=%ERRORLEVEL%
echo [stage3] Vulkan probe errorlevel=%VULKAN_STATUS% >> "%LOG%"
if "%VULKAN_STATUS%"=="0" goto :vulkan_ok
cmd /c exit /b %VULKAN_STATUS%
goto :fail

:vulkan_ok
echo [stage3] capture PnP, class-registry, DxgKrnl, and SetupAPI diagnostics >> "%LOG%"
if exist C:\BridgeVM\bvgpu-diagnostics.ps1 powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-diagnostics.ps1 >> "%LOG%" 2>&1
echo [stage3] verify PnP status and bound viogpu3d INF >> "%LOG%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$expectedInf = 'C:\BridgeVM\viogpu3d\viogpu3d.inf'; $dev = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object { $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_(1050|10F7)(?:&|$)' -and $_.Status -eq 'OK' } | Select-Object -First 1; if (-not $dev) { Write-Error 'VirtIO GPU device is not present with Status OK'; exit 1 }; $drv = Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceID -eq $dev.InstanceId } | Select-Object -First 1; if (-not $drv -or $drv.InfName -notmatch '^oem[0-9]+[.]inf$') { Write-Error 'VirtIO GPU is not bound to an OEM driver package'; exit 2 }; $boundInf = Join-Path $env:windir ('INF\' + $drv.InfName); if (-not (Test-Path -LiteralPath $expectedInf -PathType Leaf) -or -not (Test-Path -LiteralPath $boundInf -PathType Leaf)) { Write-Error ('Expected or bound INF is missing: expected=' + $expectedInf + ' bound=' + $boundInf); exit 3 }; $expectedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $expectedInf).Hash; $boundHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $boundInf).Hash; if ($boundHash -ne $expectedHash) { Write-Error ('Bound OEM INF does not match injected viogpu3d INF: bound=' + $boundInf + ' bound_sha256=' + $boundHash + ' expected_sha256=' + $expectedHash); exit 4 }; $dev | Format-List Status,Class,FriendlyName,InstanceId; $drv | Format-List DeviceName,DriverVersion,DriverProviderName,InfName; Write-Output ('expected_inf_sha256=' + $expectedHash); Write-Output ('bound_inf_sha256=' + $boundHash)" >> "%LOG%" 2>&1
if errorlevel 1 goto :fail
echo [stage3] active refresh rate >> "%LOG%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$v=Get-CimInstance Win32_VideoController; Write-Output ('refresh CUR=' + $v.CurrentRefreshRate + ' MAX=' + $v.MaxRefreshRate)" >> "%LOG%" 2>&1
echo [stage3] delete continuation task >> "%LOG%"
schtasks /Delete /TN "%TASK_NAME%" /F >> "%LOG%" 2>&1
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
exit /b 0

:write_boot_identity
call :read_boot_identity
if errorlevel 1 exit /b 1
> "%~1" echo %CURRENT_BOOT_ID%
if errorlevel 1 exit /b 1
echo [boot-identity] path=%~1 value=%CURRENT_BOOT_ID% >> "%LOG%"
exit /b %ERRORLEVEL%

:require_new_boot
if not exist "%~1" (
  echo [boot-gate] previous boot identity is missing: %~1 >> "%LOG%"
  exit /b 1
)
set "PREVIOUS_BOOT_ID="
set /p PREVIOUS_BOOT_ID=<"%~1"
if not defined PREVIOUS_BOOT_ID (
  echo [boot-gate] previous boot identity is empty: %~1 >> "%LOG%"
  exit /b 1
)
call :read_boot_identity
if errorlevel 1 exit /b 1
if /i "%CURRENT_BOOT_ID%"=="%PREVIOUS_BOOT_ID%" (
  echo [boot-gate] stage transition requires a completed reboot: boot_identity=%CURRENT_BOOT_ID% >> "%LOG%"
  exit /b 1
)
echo [boot-gate] previous=%PREVIOUS_BOOT_ID% current=%CURRENT_BOOT_ID% >> "%LOG%"
exit /b 0

:read_boot_identity
set "CURRENT_BOOT_ID="
for /f "tokens=1,2,3" %%I in ('reg query "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters" /v BootId 2^>nul') do if /i "%%I"=="BootId" set "CURRENT_BOOT_ID=%%K"
if not defined CURRENT_BOOT_ID (
  echo [boot-identity] failed to read PrefetchParameters\BootId >> "%LOG%"
  exit /b 1
)
exit /b 0
