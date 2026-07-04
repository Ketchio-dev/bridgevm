@echo off
rem BridgeVM WinPE scripted install — replaces setup.exe via winpeshl.ini.
rem Applies the split install.swm from the NSID-1 FAT32 installer onto the
rem NSID-2 target, then writes a UEFI boot entry. Keyboard-free.
echo BVINSTALL START
wpeinit

rem --- locate the installer source drive (has \sources\install.swm) ---
set SRC=
for %%D in (C D E F G H I) do if exist %%D:\sources\install.swm set SRC=%%D:
if "%SRC%"=="" (
  echo BVINSTALL ERROR: install.swm source not found
  goto :end
)
echo SOURCE DRIVE=%SRC%

rem --- partition the target (NSID-2 = disk 1) ---
echo BVINSTALL DISKPART
diskpart /s %SYSTEMROOT%\System32\bvdiskpart.txt
if errorlevel 1 (
  echo BVINSTALL ERROR: diskpart failed
  goto :end
)

rem --- apply the split WIM to W: ---
echo BVINSTALL DISM APPLY
dism /apply-image /imagefile:%SRC%\sources\install.swm /swmfile:%SRC%\sources\install*.swm /index:1 /applydir:W:\
if errorlevel 1 (
  echo BVINSTALL ERROR: dism apply failed
  goto :end
)

rem --- write the UEFI boot files onto the ESP (S:) ---
echo BVINSTALL BCDBOOT
bcdboot W:\Windows /s S: /f UEFI
if errorlevel 1 (
  echo BVINSTALL ERROR: bcdboot failed
  goto :end
)

rem --- plant the OOBE-skip/autologon unattend into the installed image ---
if exist %SRC%\unattend.xml (
  echo BVINSTALL UNATTEND
  if not exist W:\Windows\Panther mkdir W:\Windows\Panther
  copy /y %SRC%\unattend.xml W:\Windows\Panther\unattend.xml
)

echo BVINSTALL DONE
:end
wpeutil shutdown
