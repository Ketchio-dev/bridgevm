@echo off
rem BridgeVM real-title demo: launch PPSSPP (a real, widely-used app with a
rem native ARM64 build) with its portable config forced to Vulkan, so its UI
rem renders directly through the virtio Venus ICD -> MoltenVK. Runs at
rem interactive logon; waits for the firstboot task to self-delete so only
rem the final boot generation launches it.
set BV_TRIES=0
:waitloop
schtasks /Query /TN "BridgeVM-VioGpu3DFirstBoot" >nul 2>&1
if errorlevel 1 goto :run
set /a BV_TRIES=BV_TRIES+1
if %BV_TRIES% GEQ 40 exit /b 0
ping -n 6 127.0.0.1 >nul
goto :waitloop

:run
set VK_DRIVER_FILES=C:\BridgeVM\viogpu3d\virtio_icd.arm64.json
set DXVK_LOG_LEVEL=info
set DXVK_LOG_PATH=C:\BridgeVM\apps\ppsspp
set VK_INSTANCE_LAYERS=
cd /d C:\BridgeVM\apps\ppsspp
taskkill /f /im PPSSPPWindowsARM64.exe >nul 2>&1
ping -n 2 127.0.0.1 >nul
if exist C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\FailedGraphicsBackends.txt del /f /q C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\FailedGraphicsBackends.txt
if exist C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM\FailedGraphicsBackends.txt del /f /q C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM\FailedGraphicsBackends.txt
copy /y C:\BridgeVM\apps\ppsspp\bv-ppsspp.ini C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\ppsspp.ini >nul
echo launch_utc=%DATE% %TIME% > C:\BridgeVM\apps\ppsspp\bv-launch.log
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-title-gate.ps1 -ManifestPath C:\BridgeVM\apps\ppsspp\bv-ppsspp-title.json
set BV_GATE_RESULT=%ERRORLEVEL%
if %BV_GATE_RESULT% EQU 0 exit /b 0
echo retry_utc=%DATE% %TIME% first_exit=%BV_GATE_RESULT% >> C:\BridgeVM\apps\ppsspp\bv-launch.log
taskkill /f /im PPSSPPWindowsARM64.exe >nul 2>&1
ping -n 6 127.0.0.1 >nul
if exist C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\FailedGraphicsBackends.txt del /f /q C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\FailedGraphicsBackends.txt
if exist C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM\FailedGraphicsBackends.txt del /f /q C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM\FailedGraphicsBackends.txt
copy /y C:\BridgeVM\apps\ppsspp\bv-ppsspp.ini C:\BridgeVM\apps\ppsspp\memstick\PSP\SYSTEM\ppsspp.ini >nul
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-title-gate.ps1 -ManifestPath C:\BridgeVM\apps\ppsspp\bv-ppsspp-title.json
exit /b %ERRORLEVEL%
