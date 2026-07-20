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

rem --- DWM dump-only diagnosis: enable one small per-process crash dump without
rem     touching the installed display package, firstboot state, or topology. ---
if exist %DRV%\..\bridgevm-dwm-dump-only.txt (
  echo BVINJECT DWM-DUMP-ONLY CONFIGURE
  if not exist %WIN%\BridgeVM\Dumps\ mkdir %WIN%\BridgeVM\Dumps
  reg load HKLM\BVDWMSW %WIN%\Windows\System32\config\SOFTWARE
  if errorlevel 1 (
    echo BVINJECT ERROR: DWM dump SOFTWARE hive load failed
    goto :end
  )
  reg add "HKLM\BVDWMSW\Microsoft\Windows\Windows Error Reporting\LocalDumps\dwm.exe" /v DumpFolder /t REG_EXPAND_SZ /d "C:\BridgeVM\Dumps" /f
  reg add "HKLM\BVDWMSW\Microsoft\Windows\Windows Error Reporting\LocalDumps\dwm.exe" /v DumpType /t REG_DWORD /d 1 /f
  reg unload HKLM\BVDWMSW
  echo BVINJECT DWM-DUMP-ONLY CONFIGURE DONE
  goto :end
)

rem --- Resume an already-planted viogpu3d activation after stage 1 completed
rem     but its delayed scheduled task did not run. Preserve both stage flags
rem     and the staged package; only restore the one-shot native boot service
rem     and its pending token so the next Windows boot enters stage 2. ---
if exist %DRV%\..\bridgevm-gpu-stage-continue-only.txt (
  echo BVINJECT GPU-STAGE-CONTINUE-ONLY CONFIGURE
  if not exist %WIN%\BridgeVM\stage1.flag (
    echo BVINJECT ERROR: GPU stage 1 flag is missing
    goto :end
  )
  if not exist %WIN%\BridgeVM\bvgpu-diagnostics-service.exe (
    echo BVINJECT ERROR: native GPU handoff service is missing
    goto :end
  )
  if not exist %WIN%\BridgeVM\bvgpu-diagnostics-run.cmd (
    echo BVINJECT ERROR: GPU handoff runner is missing
    goto :end
  )
  echo pending > %WIN%\BridgeVM\viogpu3d-firstboot-pending.flag
  reg load HKLM\BVGPUCONTSYS %WIN%\Windows\System32\config\SYSTEM
  if errorlevel 1 (
    echo BVINJECT ERROR: GPU continuation SYSTEM hive load failed
    goto :end
  )
  reg delete "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /f 2>nul
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Type /t REG_DWORD /d 0x10 /f
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Start /t REG_DWORD /d 0x2 /f
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ErrorControl /t REG_DWORD /d 0x1 /f
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ImagePath /t REG_EXPAND_SZ /d "C:\BridgeVM\bvgpu-diagnostics-service.exe" /f
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v DisplayName /t REG_SZ /d "BridgeVM viogpu3d activation continuation" /f
  reg add "HKLM\BVGPUCONTSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ObjectName /t REG_SZ /d "LocalSystem" /f
  reg unload HKLM\BVGPUCONTSYS
  echo BVINJECT GPU-STAGE-CONTINUE-ONLY CONFIGURE DONE
  goto :end
)

rem --- Diagnostics-only helpers must not perturb a proven driver binding.
rem     Copy the probes and register one live-Windows RunOnce, then leave before
rem     DISM, testsigning, display-topology reset, or agent mutation. ---
if exist %DRV%\..\bridgevm-diagnostics-only.txt (
  echo BVINJECT DIAGNOSTICS-ONLY PLANT
  if not exist %WIN%\BridgeVM\ mkdir %WIN%\BridgeVM
  if not exist %DRV%\..\bvgpu-diagnostics.ps1 (
    echo BVINJECT ERROR: diagnostics script missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-vulkan-probe.ps1 (
    echo BVINJECT ERROR: Vulkan probe missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-diagnostics-run.cmd (
    echo BVINJECT ERROR: diagnostics runner missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-diagnostics-service.exe (
    echo BVINJECT ERROR: native diagnostics service missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-d3dkmt-probe.exe (
    echo BVINJECT ERROR: D3DKMT diagnostics probe missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-diagnostics-startup.cmd (
    echo BVINJECT ERROR: diagnostics Startup launcher missing
    goto :end
  )
  copy /y %DRV%\..\bvgpu-diagnostics.ps1 %WIN%\BridgeVM\bvgpu-diagnostics.ps1 >nul
  copy /y %DRV%\..\bvgpu-vulkan-probe.ps1 %WIN%\BridgeVM\bvgpu-vulkan-probe.ps1 >nul
  copy /y %DRV%\..\bvgpu-diagnostics-run.cmd %WIN%\BridgeVM\bvgpu-diagnostics-run.cmd >nul
  copy /y %DRV%\..\bvgpu-diagnostics-service.exe %WIN%\BridgeVM\bvgpu-diagnostics-service.exe >nul
  copy /y %DRV%\..\bvgpu-d3dkmt-probe.exe %WIN%\BridgeVM\bvgpu-d3dkmt-probe.exe >nul
  if not exist "%WIN%\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup\" mkdir "%WIN%\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup"
  copy /y %DRV%\..\bvgpu-diagnostics-startup.cmd "%WIN%\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup\BridgeVMGpuDiagnostics.cmd" >nul
  del /f /q %WIN%\BridgeVM\bvgpu-diagnostics-latest.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-vulkan-probe.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-vulkan-probe.log.console 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-diagnostics-complete.flag 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-service-entry.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-native-service-entry.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-runner-entry.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-mesa-vulkan.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-d3dkmt-probe.log 2>nul
  rmdir %WIN%\BridgeVM\bvgpu-diagnostics-running 2>nul
  reg load HKLM\BVDIAGSW %WIN%\Windows\System32\config\SOFTWARE
  if errorlevel 1 (
    echo BVINJECT ERROR: diagnostics SOFTWARE hive load failed
    goto :end
  )
  rem Run is used instead of RunOnce because this image's autologon shell did
  rem not consume a correctly planted HKLM RunOnce value. The runner deletes
  rem this Run value before doing any work and has a filesystem race guard.
  reg add "HKLM\BVDIAGSW\Microsoft\Windows\CurrentVersion\Run" /v BridgeVMGpuDiagnostics /t REG_SZ /d "cmd.exe /c C:\BridgeVM\bvgpu-diagnostics-run.cmd" /f
  reg unload HKLM\BVDIAGSW
  rem This already-logged-on image does not consume newly planted Run or
  rem Startup entries.  Add a one-shot boot service as the session-independent
  rem handoff.  SCM still launches cmd.exe before it reports that cmd is not a
  rem service binary; the runner deletes this key immediately, then captures
  rem the diagnostics from LocalSystem session 0.
  reg load HKLM\BVDIAGSYS %WIN%\Windows\System32\config\SYSTEM
  if errorlevel 1 (
    echo BVINJECT ERROR: diagnostics SYSTEM hive load failed
    goto :end
  )
  rem Remove names used by older diagnostics injectors so they cannot race the
  rem delayed, self-marking handoff below.
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnostics" /f 2>nul
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe" /f 2>nul
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe2" /f 2>nul
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe3" /f 2>nul
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe4" /f 2>nul
  reg delete "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe5" /f 2>nul
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Type /t REG_DWORD /d 0x10 /f
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Start /t REG_DWORD /d 0x2 /f
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ErrorControl /t REG_DWORD /d 0x1 /f
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ImagePath /t REG_EXPAND_SZ /d "C:\BridgeVM\bvgpu-diagnostics-service.exe" /f
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v DisplayName /t REG_SZ /d "BridgeVM one-shot GPU diagnostics" /f
  reg add "HKLM\BVDIAGSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ObjectName /t REG_SZ /d "LocalSystem" /f
  reg unload HKLM\BVDIAGSYS
  echo BVINJECT DIAGNOSTICS-ONLY PLANT DONE
  goto :end
)

rem --- inject every driver under \drivers into the offline image.  A deliberate
rem     reinjection marker bypasses DISM when an installed display package is
rem     being superseded: live firstboot pnputil below performs the authoritative
rem     staging and binding after the package/certificate have been planted. ---
if exist %DRV%\..\bridgevm-skip-offline-dism.txt (
  echo BVINJECT DISM SKIPPED FOR LIVE ACTIVATION
) else (
  echo BVINJECT DISM ADD-DRIVER
  dism /Image:%WIN%\ /Add-Driver /Driver:%DRV% /Recurse
  if errorlevel 1 (
    echo BVINJECT ERROR: dism add-driver failed
    goto :end
  )

  echo BVINJECT VERIFY
  dism /Image:%WIN%\ /Get-Drivers | find /i "oem"
)

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
rem     (on first boot) only lets the kernel LOAD test-signed code once installed; it does
rem     not make PnP TRUST the publisher for install. So copy the package + cert +
rem     activation script to C:\BridgeVM and register a LocalSystem boot service
rem     plus an elevated HKLM RunOnce fallback. On first boot it imports the cert into the machine Root +
rem     TrustedPublisher stores and forces `pnputil /add-driver /install` to bind
rem     PCI\VEN_1AF4&DEV_1050 or PCI\VEN_1AF4&DEV_10F7, as declared by the package.
rem     UAC is disabled so RunOnce gets the full admin
rem     token; output is logged to C:\BridgeVM\viogpu3d-firstboot.log. ---
if exist %DRV%\viogpu3d\viogpu3d.inf if exist %DRV%\..\bvgpu-firstboot.cmd (
  echo BVINJECT VIOGPU3D FIRSTBOOT PLANT
  if not exist %DRV%\..\bvgpu-diagnostics-run.cmd (
    echo BVINJECT ERROR: firstboot service runner missing
    goto :end
  )
  if not exist %DRV%\..\bvgpu-diagnostics-service.exe (
    echo BVINJECT ERROR: firstboot native service missing
    goto :end
  )
  if not exist %WIN%\BridgeVM\viogpu3d\ mkdir %WIN%\BridgeVM\viogpu3d
  rem Reinjection means a new package/activation attempt. Never let markers
  rem from an older package skip the trust or bind stages.
  del /f /q %WIN%\BridgeVM\stage1.flag 2>nul
  del /f /q %WIN%\BridgeVM\stage2.flag 2>nul
  del /f /q %WIN%\BridgeVM\stage1.boot 2>nul
  del /f /q %WIN%\BridgeVM\stage2.boot 2>nul
  del /f /q %WIN%\BridgeVM\gpu-rebooted.flag 2>nul
  del /f /q %WIN%\BridgeVM\viogpu3d-firstboot.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-native-service-entry.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-runner-entry.log 2>nul
  del /f /q %WIN%\BridgeVM\bvgpu-vulkan-draw.log 2>nul
  copy /y %DRV%\viogpu3d\* %WIN%\BridgeVM\viogpu3d\ >nul
  copy /y %DRV%\..\bvgpu-firstboot.cmd %WIN%\BridgeVM\bvgpu-firstboot.cmd >nul
  copy /y %DRV%\..\bvgpu-diagnostics-run.cmd %WIN%\BridgeVM\bvgpu-diagnostics-run.cmd >nul
  copy /y %DRV%\..\bvgpu-diagnostics-service.exe %WIN%\BridgeVM\bvgpu-diagnostics-service.exe >nul
  echo pending > %WIN%\BridgeVM\viogpu3d-firstboot-pending.flag
  if exist %DRV%\..\bvgpu-diagnostics.ps1 copy /y %DRV%\..\bvgpu-diagnostics.ps1 %WIN%\BridgeVM\bvgpu-diagnostics.ps1 >nul
  if exist %DRV%\..\bvgpu-vulkan-probe.ps1 copy /y %DRV%\..\bvgpu-vulkan-probe.ps1 %WIN%\BridgeVM\bvgpu-vulkan-probe.ps1 >nul
  if exist %DRV%\..\bvgpu-vulkan-draw-smoke.exe copy /y %DRV%\..\bvgpu-vulkan-draw-smoke.exe %WIN%\BridgeVM\bvgpu-vulkan-draw-smoke.exe >nul
  reg load HKLM\BVGPUSW %WIN%\Windows\System32\config\SOFTWARE
  rem RunOnce runs once at first interactive logon. The value uses the RUNTIME
  rem path (installed Windows is C: to itself), not the WinPE %WIN% letter. The
  rem "!" name prefix keeps the initial entry until stage 1 finishes creating
  rem the persistent delayed ONSTART continuation task.
  reg add "HKLM\BVGPUSW\Microsoft\Windows\CurrentVersion\RunOnce" /v !BridgeVMGpu3DStage1 /t REG_SZ /d "cmd /c C:\BridgeVM\bvgpu-firstboot.cmd" /f
  rem Ensure the admin autologon gets an un-filtered token so certutil/pnputil in
  rem the RunOnce run elevated (idempotent with the agent block below).
  reg add "HKLM\BVGPUSW\Microsoft\Windows\CurrentVersion\Policies\System" /v EnableLUA /t REG_DWORD /d 0 /f
  reg unload HKLM\BVGPUSW
  rem The image used for bring-up does not reliably consume newly planted
  rem RunOnce values. Register the native one-shot service as the primary,
  rem session-independent LocalSystem handoff. The runner deletes it after a
  rem successful stage 1; the scheduled ONSTART task then owns stages 2 and 3.
  reg load HKLM\BVGPUSYS %WIN%\Windows\System32\config\SYSTEM
  if errorlevel 1 (
    echo BVINJECT ERROR: firstboot SYSTEM hive load failed
    goto :end
  )
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnostics" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe2" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe3" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe4" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe5" /f 2>nul
  reg delete "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /f 2>nul
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Type /t REG_DWORD /d 0x10 /f
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v Start /t REG_DWORD /d 0x2 /f
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ErrorControl /t REG_DWORD /d 0x1 /f
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ImagePath /t REG_EXPAND_SZ /d "C:\BridgeVM\bvgpu-diagnostics-service.exe" /f
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v DisplayName /t REG_SZ /d "BridgeVM one-shot viogpu3d activation" /f
  reg add "HKLM\BVGPUSYS\ControlSet001\Services\BridgeVMGpuDiagnosticsProbe6" /v ObjectName /t REG_SZ /d "LocalSystem" /f
  rem Recovery path for a currently bound viogpu3d that crashes before this
  rem activation service can run. Disable only the existing driver service for
  rem the next boot. Stage-2 pnputil processes the replacement INF's AddService
  rem section and restores VioGpu3D to SERVICE_DEMAND_START before rebooting.
  if exist %DRV%\..\bridgevm-quarantine-viogpu3d.txt (
    echo BVINJECT VIOGPU3D BOOT QUARANTINE
    reg query "HKLM\BVGPUSYS\ControlSet001\Services\VioGpu3D" >nul 2>&1
    if not errorlevel 1 reg add "HKLM\BVGPUSYS\ControlSet001\Services\VioGpu3D" /v Start /t REG_DWORD /d 0x4 /f
    reg query "HKLM\BVGPUSYS\ControlSet002\Services\VioGpu3D" >nul 2>&1
    if not errorlevel 1 reg add "HKLM\BVGPUSYS\ControlSet002\Services\VioGpu3D" /v Start /t REG_DWORD /d 0x4 /f
    echo BVINJECT VIOGPU3D BOOT QUARANTINE DONE
  )
  reg unload HKLM\BVGPUSYS
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
