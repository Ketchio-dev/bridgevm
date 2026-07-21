@echo off
rem BridgeVM visible present demo. Launched at every interactive logon by an
rem HKLM Run entry planted by bvinject; it waits until the firstboot
rem continuation task has deleted itself (stage 3 complete), so only the
rem final boot generation actually presents - earlier generations reboot out
rem of the wait loop. The demo shows a real window on the Venus desktop and
rem presents with vsync pacing so host-side scanout samples catch the
rem magenta client area.
set BV_TRIES=0
:waitloop
schtasks /Query /TN "BridgeVM-VioGpu3DFirstBoot" >nul 2>&1
if errorlevel 1 goto :run_demo
set /a BV_TRIES=BV_TRIES+1
if %BV_TRIES% GEQ 40 exit /b 0
ping -n 6 127.0.0.1 >nul
goto :waitloop

:run_demo
set BV_PRESENT_DEMO=1
set VK_DRIVER_FILES=C:\BridgeVM\viogpu3d\virtio_icd.arm64.json
set DXVK_LOG_PATH=C:\BridgeVM\dxvk
cd /d C:\BridgeVM\dxvk
C:\BridgeVM\dxvk\bridgevm-d3d11-present-smoke.exe > C:\BridgeVM\dxvk\present-demo.log 2>&1
echo errorlevel=%ERRORLEVEL% >> C:\BridgeVM\dxvk\present-demo.log
