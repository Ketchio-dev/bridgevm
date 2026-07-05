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

echo BVINJECT DONE
:end
wpeutil shutdown
