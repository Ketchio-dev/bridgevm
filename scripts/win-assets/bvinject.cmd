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

rem --- test-signing for test-signed packages (viogpu3d) is handled LIVE by
rem     bvgpu-firstboot.cmd (`bcdedit /set {current} testsigning on` + a guarded
rem     reboot), NOT offline here: the target ESP has no drive letter in WinPE so
rem     an offline `bcdedit /store <ESP>\BCD` is unreliable and previously aborted
rem     the whole injector. The marker file (bridgevm-enable-testsigning.txt) is
rem     retained only as an intent flag; nothing offline acts on it now. ---

rem --- viogpu3d is a TEST-SIGNED third-party display driver. Offline dism only
rem     STAGES it into the DriverStore: it neither trusts the self-signed test
rem     publisher (so silent PnP install of the package is REFUSED) nor re-runs a
rem     driver search for the already-present virtio-gpu device. BCD testsigning
rem     (above) only lets the kernel LOAD test-signed code once installed; it does
rem     not make PnP TRUST the publisher for install. So copy the package + cert +
rem     activation script to C:\BridgeVM and register an elevated HKLM RunOnce
rem     that, on first boot, imports the cert into the machine Root +
rem     TrustedPublisher stores and forces `pnputil /add-driver /install` to bind
rem     PCI\VEN_1AF4&DEV_1050. UAC is disabled so RunOnce gets the full admin
rem     token; output is logged to C:\BridgeVM\viogpu3d-firstboot.log. ---
if exist %DRV%\viogpu3d\viogpu3d.inf if exist %DRV%\..\bvgpu-firstboot.cmd (
  echo BVINJECT VIOGPU3D FIRSTBOOT PLANT
  if not exist %WIN%\BridgeVM\viogpu3d\ mkdir %WIN%\BridgeVM\viogpu3d
  rem Reinjection means a new package/activation attempt. Never let markers
  rem from an older package skip the trust or bind stages.
  del /f /q %WIN%\BridgeVM\stage1.flag 2>nul
  del /f /q %WIN%\BridgeVM\stage2.flag 2>nul
  del /f /q %WIN%\BridgeVM\gpu-rebooted.flag 2>nul
  del /f /q %WIN%\BridgeVM\viogpu3d-firstboot.log 2>nul
  copy /y %DRV%\viogpu3d\* %WIN%\BridgeVM\viogpu3d\ >nul
  copy /y %DRV%\..\bvgpu-firstboot.cmd %WIN%\BridgeVM\bvgpu-firstboot.cmd >nul
  reg load HKLM\BVGPUSW %WIN%\Windows\System32\config\SOFTWARE
  rem RunOnce runs once at first interactive logon. The value uses the RUNTIME
  rem path (installed Windows is C: to itself), not the WinPE %WIN% letter. The
  rem "!" name prefix defers deletion until the command completes, so a reboot
  rem mid-activation retries instead of silently dropping it.
  reg add "HKLM\BVGPUSW\Microsoft\Windows\CurrentVersion\RunOnce" /v !BridgeVMGpu3DStage1 /t REG_SZ /d "cmd /c C:\BridgeVM\bvgpu-firstboot.cmd" /f
  rem Ensure the admin autologon gets an un-filtered token so certutil/pnputil in
  rem the RunOnce run elevated (idempotent with the agent block below).
  reg add "HKLM\BVGPUSW\Microsoft\Windows\CurrentVersion\Policies\System" /v EnableLUA /t REG_DWORD /d 0 /f
  reg unload HKLM\BVGPUSW
  echo BVINJECT VIOGPU3D FIRSTBOOT PLANT DONE
)

rem --- when injecting the virtio-gpu display driver, reset the persisted
rem     display topology so Windows re-detects monitors on next boot and makes
rem     the (now sole) virtio-gpu adapter primary — otherwise the taskbar stays
rem     assigned to the removed Basic Display and never renders. Harmless: the
rem     GraphicsDrivers Configuration/Connectivity keys are rebuilt on boot. ---
set GPUDISP=
if exist %DRV%\viogpudo\viogpudo.inf set GPUDISP=1
if exist %DRV%\viogpu3d\viogpu3d.inf set GPUDISP=1
if defined GPUDISP (
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
  rem The first user is an Administrator but UAC filters its token, so a
  rem Run-launched agent runs unelevated and CANNOT open the virtio-serial
  rem port (access denied). Disable UAC (dev/build VM) so the agent gets the
  rem full admin token and can open the port on first boot.
  reg add "HKLM\BVSW\Microsoft\Windows\CurrentVersion\Policies\System" /v EnableLUA /t REG_DWORD /d 0 /f
  rem Disable the guest power plan ONCE at first logon (RunOnce). The default
  rem plan sleeps the VM at desktop+5min, which freezes the guest agent and
  rem kills the host service channel (live-diagnosed as a hard t~360s wall in
  rem every clipboard/share soak until powercfg was applied over the channel).
  rem The ampersands are inside the quoted /d string, so cmd passes them to
  rem the RunOnce value literally and they chain at first-logon execution.
  reg add "HKLM\BVSW\Microsoft\Windows\CurrentVersion\RunOnce" /v BridgeVMPower /t REG_SZ /d "cmd.exe /c powercfg /change standby-timeout-ac 0 & powercfg /change standby-timeout-dc 0 & powercfg /change monitor-timeout-ac 0 & powercfg /change monitor-timeout-dc 0 & powercfg /h off" /f
  reg unload HKLM\BVSW
  rem NOTE: exactly ONE autostart. We deliberately do NOT also drop a Startup
  rem launcher: two autostarts race two agent instances, and the loser's
  rem CreateFile/close churns vioser's single-open port (resetting
  rem HostConnected) every few seconds. The HKLM Run key above is the single
  rem source of truth; bvagent.ps1's mutex is only a belt-and-suspenders guard.
  echo BVINJECT AGENT PLANT DONE
)

echo BVINJECT DONE
:end
wpeutil shutdown
