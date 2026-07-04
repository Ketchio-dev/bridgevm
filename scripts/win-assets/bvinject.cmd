@echo off
rem BridgeVM WinPE offline driver injector — runs instead of setup.exe.
rem Finds the installed Windows volume (NSID-2 target) and the netkvm driver
rem on the injector source, adds the driver to the offline image, shuts down.
echo BVINJECT START
wpeinit

rem --- locate the injector source (has \drivers\netkvm\netkvm.inf) ---
set DRV=
for %%D in (C D E F G H I J) do if exist %%D:\drivers\netkvm\netkvm.inf set DRV=%%D:\drivers\netkvm
if "%DRV%"=="" (
  echo BVINJECT ERROR: netkvm driver not found on source
  goto :end
)
echo DRIVER DIR=%DRV%

rem --- locate the installed Windows volume (NSID-2 target) ---
set WIN=
for %%D in (C D E F G H I J) do if exist %%D:\Windows\System32\ntoskrnl.exe set WIN=%%D:
if "%WIN%"=="" (
  echo BVINJECT ERROR: installed Windows volume not found
  goto :end
)
echo WINDOWS VOLUME=%WIN%

rem --- inject the driver into the offline image ---
echo BVINJECT DISM ADD-DRIVER
dism /Image:%WIN%\ /Add-Driver /Driver:%DRV% /Recurse
if errorlevel 1 (
  echo BVINJECT ERROR: dism add-driver failed
  goto :end
)

echo BVINJECT VERIFY
dism /Image:%WIN%\ /Get-Drivers | find /i "netkvm"

echo BVINJECT DONE
:end
wpeutil shutdown
