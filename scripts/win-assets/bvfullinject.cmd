@echo off
echo BVFULL START
wpeinit
set SRC=
for %%D in (C D E F G H I J) do if exist %%D:\drivers\viogpu3d\viogpu3d.inf set SRC=%%D:
if "%SRC%"=="" ( echo BVFULL ERROR: driver source not found & goto :end )
set WIN=
for %%D in (C D E F G H I J) do if exist %%D:\Windows\System32\ntoskrnl.exe set WIN=%%D:
if "%WIN%"=="" ( echo BVFULL ERROR: Windows volume not found & goto :end )
echo SRC=%SRC% WIN=%WIN%
echo BVFULL DISM ADD-DRIVER (vioserial netkvm viogpu3d)
dism /Image:%WIN%\ /Add-Driver /Driver:%SRC%\drivers /Recurse
echo BVFULL dism exit=%errorlevel%
if not exist %WIN%\BridgeVM\viogpu3d\ mkdir %WIN%\BridgeVM\viogpu3d
copy /y %SRC%\drivers\viogpu3d\* %WIN%\BridgeVM\viogpu3d\ >nul
copy /y %SRC%\bvbind3.cmd %WIN%\BridgeVM\bvbind3.cmd >nul
del /f /q %WIN%\BridgeVM\bvbind3.done 2>nul
copy /y %SRC%\bvagent.ps1 %WIN%\bvagent.ps1 >nul
echo BVFULL PLANT RUNONCE + AGENT + EnableLUA
reg load HKLM\BVF %WIN%\Windows\System32\config\SOFTWARE
reg add "HKLM\BVF\Microsoft\Windows\CurrentVersion\RunOnce" /v BridgeVMFullBind /t REG_SZ /d "cmd /c C:\BridgeVM\bvbind3.cmd" /f
reg add "HKLM\BVF\Microsoft\Windows\CurrentVersion\Run" /v BridgeVMAgent /t REG_SZ /d "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\bvagent.ps1" /f
reg add "HKLM\BVF\Microsoft\Windows\CurrentVersion\Policies\System" /v EnableLUA /t REG_DWORD /d 0 /f
reg unload HKLM\BVF
echo BVFULL DISPLAY-CONFIG RESET
reg load HKLM\BVFS %WIN%\Windows\System32\config\SYSTEM
reg delete "HKLM\BVFS\ControlSet001\Control\GraphicsDrivers\Configuration" /f 2>nul
reg delete "HKLM\BVFS\ControlSet001\Control\GraphicsDrivers\Connectivity" /f 2>nul
reg unload HKLM\BVFS
echo BVFULL DONE
:end
wpeutil shutdown
