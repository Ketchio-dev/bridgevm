@echo off
set LOG=C:\BridgeVM\bvbind.log
echo [bvbind] invoked %DATE% %TIME% >> "%LOG%"
rem stage1 already did: testsigning on + cert trusted. Remove the fragile CIM-gated
rem firstboot task so it stops hanging, then just bind the driver and reboot once.
schtasks /Delete /TN "BridgeVM-VioGpu3DFirstBoot" /F >> "%LOG%" 2>&1
if exist C:\BridgeVM\bvbind.done goto :verify
echo [bvbind] pnputil add-driver /install >> "%LOG%"
pnputil /add-driver C:\BridgeVM\viogpu3d\viogpu3d.inf /install >> "%LOG%" 2>&1
echo [bvbind] pnputil exit=%errorlevel% >> "%LOG%"
pnputil /scan-devices >> "%LOG%" 2>&1
echo [bvbind] reset display config for 120Hz >> "%LOG%"
reg delete "HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\Configuration" /f >> "%LOG%" 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\Connectivity" /f >> "%LOG%" 2>&1
echo done > C:\BridgeVM\bvbind.done
echo [bvbind] rebooting to start viogpu3d >> "%LOG%"
shutdown /r /t 5 /c "BridgeVM viogpu3d bind"
goto :end
:verify
echo [bvbind] already bound; verify >> "%LOG%"
powershell -NoProfile -Command "Get-PnpDevice -PresentOnly | Where-Object {$_.InstanceId -match 'VEN_1AF4&DEV_1050'} | Format-List Status,FriendlyName" >> "%LOG%" 2>&1
:end
