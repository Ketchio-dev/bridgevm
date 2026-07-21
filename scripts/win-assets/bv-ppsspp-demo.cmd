@echo off
rem BridgeVM real-title demo: launch PPSSPP (a real, widely-used app with a
rem native ARM64 build and a D3D11 backend) with the DXVK d3d11/dxgi beside
rem its exe, so its UI renders through DXVK -> Venus -> MoltenVK. Runs at
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
echo launch_utc=%DATE% %TIME% > C:\BridgeVM\apps\ppsspp\bv-launch.log
start "" C:\BridgeVM\apps\ppsspp\PPSSPPWindowsARM64.exe
