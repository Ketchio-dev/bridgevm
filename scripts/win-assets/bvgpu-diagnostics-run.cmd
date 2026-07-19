@echo off
setlocal
set DIAG_LOG=C:\BridgeVM\bvgpu-diagnostics-latest.log
set VULKAN_LOG=C:\BridgeVM\bvgpu-vulkan-probe.log
set RUNNING_DIR=C:\BridgeVM\bvgpu-diagnostics-running
set COMPLETE_FLAG=C:\BridgeVM\bvgpu-diagnostics-complete.flag
set FIRSTBOOT_PENDING=C:\BridgeVM\viogpu3d-firstboot-pending.flag
echo [diagnostics-run] entered_utc=%DATE% %TIME% > C:\BridgeVM\bvgpu-runner-entry.log

rem A normal viogpu3d injection uses this native service as a session-independent
rem firstboot handoff.  Run stage 1 as LocalSystem before touching the separate
rem diagnostics-only registrations.  A successful stage 1 creates its flag and
rem schedules the persistent ONSTART task that owns stages 2 and 3.
if exist "%FIRSTBOOT_PENDING%" (
  call C:\BridgeVM\bvgpu-firstboot.cmd
  if not exist C:\BridgeVM\stage1.flag exit /b 1
  del /f /q "%FIRSTBOOT_PENDING%" >nul 2>&1
  sc.exe delete BridgeVMGpuDiagnosticsProbe6 >nul 2>&1
  exit /b 0
)

rem Make the persistent HKLM Run handoff one-shot before invoking PowerShell.
rem The directory creation is atomic enough to reject a simultaneous legacy
rem RunOnce launch left by an older diagnostics injector.
reg delete "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v BridgeVMGpuDiagnostics /f >nul 2>&1
del /f /q "C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup\BridgeVMGpuDiagnostics.cmd" >nul 2>&1
sc.exe delete BridgeVMGpuDiagnosticsProbe6 >nul 2>&1
if exist "%COMPLETE_FLAG%" exit /b 0
if exist "%RUNNING_DIR%" exit /b 0
mkdir "%RUNNING_DIR%" 2>nul
if not exist "%RUNNING_DIR%" exit /b 0

echo [diagnostics-run] start_utc=%DATE% %TIME% > "%DIAG_LOG%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-diagnostics.ps1 >> "%DIAG_LOG%" 2>&1
set DIAG_RC=%ERRORLEVEL%
echo [diagnostics-run] diagnostics_exit_code=%DIAG_RC% >> "%DIAG_LOG%"

C:\BridgeVM\bvgpu-d3dkmt-probe.exe
set D3DKMT_RC=%ERRORLEVEL%
echo [diagnostics-run] d3dkmt_probe_exit_code=%D3DKMT_RC% >> "%DIAG_LOG%"

set VN_DEBUG=init,result
set MESA_LOG_FILE=C:\BridgeVM\bvgpu-mesa-vulkan.log
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\bvgpu-vulkan-probe.ps1 > "%VULKAN_LOG%" 2>&1
set VULKAN_RC=%ERRORLEVEL%
echo [diagnostics-run] vulkan_exit_code=%VULKAN_RC% >> "%VULKAN_LOG%"
echo [diagnostics-run] complete_utc=%DATE% %TIME% >> "%DIAG_LOG%"
echo done > "%COMPLETE_FLAG%"
rmdir "%RUNNING_DIR%" 2>nul
exit /b %VULKAN_RC%
