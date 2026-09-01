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

rem --- require the host-verified signed ARM64 storage/serial/network payload ---
set PROVISION=%SRC%\bridgevm\provisioning
if not exist %PROVISION%\payload-receipt.tsv (
  echo BVINSTALL BLOCKER: required signed ARM64 guest payload missing
  goto :end
)
if not exist %PROVISION%\drivers\* (
  echo BVINSTALL BLOCKER: verified guest driver tree missing
  goto :end
)

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

rem --- stage catalog-signed drivers offline; never force unsigned packages ---
echo BVINSTALL DRIVER PROVISION START roles=storage,serial,network
dism /image:W:\ /add-driver /driver:"%PROVISION%\drivers" /recurse
set DRIVER_RC=%ERRORLEVEL%
if "%DRIVER_RC%"=="0" goto :drivers_ok
if "%DRIVER_RC%"=="3010" goto :drivers_ok
echo BVINSTALL BLOCKER: signed guest driver provisioning failed rc=%DRIVER_RC%
goto :end
:drivers_ok
echo BVINSTALL DRIVER PROVISION STAGED live-bind-not-yet-proven

rem --- copy the sealed receipt and BridgeVM-owned agent for first logon ---
if not exist %PROVISION%\agent\bvagent.ps1 (
  echo BVINSTALL BLOCKER: BridgeVM guest agent missing
  goto :end
)
if not exist %PROVISION%\agent\bvagent-firstboot.ps1 (
  echo BVINSTALL BLOCKER: BridgeVM first-boot provisioner missing
  goto :end
)
if not exist W:\BridgeVM mkdir W:\BridgeVM
xcopy /e /i /h /y "%PROVISION%" "W:\BridgeVM\provisioning" >nul
if errorlevel 1 (
  echo BVINSTALL ERROR: guest provisioning payload copy failed
  goto :end
)
fc /b %PROVISION%\payload-receipt.tsv W:\BridgeVM\provisioning\payload-receipt.tsv >nul
if errorlevel 1 (
  echo BVINSTALL ERROR: guest provisioning receipt copy mismatch
  goto :end
)
echo BVINSTALL AGENT STAGED first-logon-ready-not-yet-proven

rem --- write the UEFI boot files onto the ESP (S:) ---
echo BVINSTALL BCDBOOT
bcdboot W:\Windows /s S: /f UEFI
if errorlevel 1 (
  echo BVINSTALL ERROR: bcdboot failed
  goto :end
)

rem --- plant the selected answer file into the installed image ---
if exist %SRC%\unattend.xml (
  echo BVINSTALL UNATTEND
  if not exist W:\Windows\Panther mkdir W:\Windows\Panther
  copy /y %SRC%\unattend.xml W:\Windows\Panther\unattend.xml
  if errorlevel 1 (
    echo BVINSTALL ERROR: unattend copy failed
    goto :end
  )
)

rem --- write the host-verifiable marker last, on the FAT32 ESP ---
if not exist S:\EFI\BridgeVM mkdir S:\EFI\BridgeVM
> S:\EFI\BridgeVM\install-success.txt echo bridgevm-windows-install-success-v1
>> S:\EFI\BridgeVM\install-success.txt echo payload-roles=storage,serial,network
>> S:\EFI\BridgeVM\install-success.txt echo bcdboot=complete
if errorlevel 1 (
  echo BVINSTALL ERROR: success marker write failed
  goto :end
)

echo BVINSTALL DONE
:end
wpeutil shutdown
