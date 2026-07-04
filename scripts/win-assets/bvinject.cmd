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

echo BVINJECT DONE
:end
wpeutil shutdown
