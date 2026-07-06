@echo off
rem BridgeVM WinPE offline driver injector — runs instead of setup.exe.
rem Finds the installed Windows volume (NSID-2 target) and the \drivers tree on
rem the injector source, adds EVERY driver under it to the offline image, and
rem shuts down. Carries any number of driver subdirs (netkvm, viogpudo, ...).
echo BVINJECT START
wpeinit

rem --- locate the injector source (has a \drivers tree with .inf files) ---
set DRV=
for %%D in (C D E F G H I J) do if exist %%D:\drivers\* set DRV=%%D:\drivers
if "%DRV%"=="" (
  echo BVINJECT ERROR: \drivers tree not found on source
  goto :end
)
echo DRIVERS DIR=%DRV%

rem --- locate the installed Windows volume (NSID-2 target) ---
set WIN=
for %%D in (C D E F G H I J) do if exist %%D:\Windows\System32\ntoskrnl.exe set WIN=%%D:
if "%WIN%"=="" (
  echo BVINJECT ERROR: installed Windows volume not found
  goto :end
)
echo WINDOWS VOLUME=%WIN%

rem --- inject every driver under \drivers into the offline image ---
echo BVINJECT DISM ADD-DRIVER
dism /Image:%WIN%\ /Add-Driver /Driver:%DRV% /Recurse
if errorlevel 1 (
  echo BVINJECT ERROR: dism add-driver failed
  goto :end
)

echo BVINJECT VERIFY
dism /Image:%WIN%\ /Get-Drivers | find /i "oem"

rem --- when injecting the virtio-gpu display driver, reset the persisted
rem     display topology so Windows re-detects monitors on next boot and makes
rem     the (now sole) virtio-gpu adapter primary — otherwise the taskbar stays
rem     assigned to the removed Basic Display and never renders. Harmless: the
rem     GraphicsDrivers Configuration/Connectivity keys are rebuilt on boot. ---
if exist %DRV%\viogpudo\viogpudo.inf (
  echo BVINJECT DISPLAY-CONFIG RESET
  reg load HKLM\BVSYS %WIN%\Windows\System32\config\SYSTEM
  reg delete "HKLM\BVSYS\ControlSet001\Control\GraphicsDrivers\Configuration" /f 2>nul
  reg delete "HKLM\BVSYS\ControlSet001\Control\GraphicsDrivers\Connectivity" /f 2>nul
  reg delete "HKLM\BVSYS\ControlSet001\Control\GraphicsDrivers\ScaleFactors" /f 2>nul
  reg unload HKLM\BVSYS
  echo BVINJECT DISPLAY-CONFIG RESET DONE
)

rem --- plant the BridgeVM guest agent (if bvagent.ps1 is on the source) and
rem     auto-start it at logon via an HKLM Run key. The image autologons to the
rem     desktop, so the agent opens the virtio-serial port shortly after boot.
rem     The agent is a pure-PowerShell command loop (no compiled binary). ---
if exist %DRV%\..\bvagent.ps1 (
  echo BVINJECT AGENT PLANT
  copy /y %DRV%\..\bvagent.ps1 %WIN%\bvagent.ps1 >nul
  reg load HKLM\BVSW %WIN%\Windows\System32\config\SOFTWARE
  rem the Run value must use the RUNTIME path (installed Windows is C: to
  rem itself), not the WinPE-assigned injection-time letter in %WIN%.
  reg add "HKLM\BVSW\Microsoft\Windows\CurrentVersion\Run" /v BridgeVMAgent /t REG_SZ /d "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\bvagent.ps1" /f
  reg unload HKLM\BVSW
  echo BVINJECT AGENT PLANT DONE
)

echo BVINJECT DONE
:end
wpeutil shutdown
