#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIRSTBOOT="scripts/win-assets/bvgpu-firstboot.cmd"
DIAGNOSTICS="scripts/win-assets/bvgpu-diagnostics.ps1"
VULKAN_PROBE="scripts/win-assets/bvgpu-vulkan-probe.ps1"
DIAGNOSTICS_RUNNER="scripts/win-assets/bvgpu-diagnostics-run.cmd"
DIAGNOSTICS_SERVICE_SOURCE="scripts/win-assets/bvgpu-diagnostics-service.c"
D3DKMT_PROBE_SOURCE="scripts/win-assets/bvgpu-d3dkmt-probe.c"
DIAGNOSTICS_STARTUP="scripts/win-assets/bvgpu-diagnostics-startup.cmd"
INJECTOR="scripts/win-assets/bvinject.cmd"
BUILD_INJECTOR="scripts/build-hvf-windows-driver-injector.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label file missing: $path"
  grep -Fq "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_file_not_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  if grep -Fq "$needle" "$path"; then
    fail "$label unexpectedly contains '$needle' in $path"
  fi
}

block_between_labels() {
  local path="$1"
  local start_label="$2"
  local end_label="$3"
  awk -v start="$start_label" -v end="$end_label" '
    $0 == start { emit = 1 }
    emit { print }
    $0 == end { exit }
  ' "$path"
}

assert_block_contains() {
  local block="$1"
  local needle="$2"
  local label="$3"
  case "$block" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $block" ;;
  esac
}

assert_block_not_contains() {
  local block="$1"
  local needle="$2"
  local label="$3"
  case "$block" in
    *"$needle"*) fail "$label unexpectedly contains '$needle'; got: $block" ;;
  esac
}

assert_block_before() {
  local block="$1"
  local first="$2"
  local second="$3"
  local label="$4"
  local first_line second_line
  first_line="$(grep -nF "$first" <<<"$block" | head -n 1 | cut -d: -f1)"
  second_line="$(grep -nF "$second" <<<"$block" | head -n 1 | cut -d: -f1)"
  [[ -n "$first_line" && -n "$second_line" && "$first_line" -lt "$second_line" ]] \
    || fail "$label expected '$first' before '$second'"
}

assert_file_contains "$FIRSTBOOT" "THREE-STAGE" "firstboot contract"
assert_file_contains "$FIRSTBOOT" "set TASK_NAME=BridgeVM-VioGpu3DFirstBoot" "firstboot scheduled task"
assert_file_contains "$FIRSTBOOT" "-File C:\BridgeVM\bvgpu-diagnostics.ps1" "firstboot diagnostics invocation"
assert_file_contains "$FIRSTBOOT" "-File C:\BridgeVM\bvgpu-vulkan-probe.ps1" "firstboot Vulkan probe invocation"
assert_file_contains "$FIRSTBOOT" "set VN_DEBUG=init,result" "firstboot Vulkan renderer debug logging"
assert_file_contains "$FIRSTBOOT" "set MESA_LOG_FILE=C:\BridgeVM\bvgpu-mesa-vulkan.log" "firstboot Mesa log capture"
assert_file_contains "$DIAGNOSTICS" "DEVPKEY_Device_ProblemCode" "diagnostics PnP problem code"
assert_file_contains "$DIAGNOSTICS" "CurrentControlSet\\Enum\\PCI" "diagnostics direct PnP registry lookup"
assert_file_contains "$DIAGNOSTICS" "VulkanDriverName" "diagnostics Vulkan registration"
assert_file_contains "$DIAGNOSTICS" "Microsoft-Windows-DxgKrnl" "diagnostics DxgKrnl events"
assert_file_contains "$DIAGNOSTICS" "setupapi.dev.log" "diagnostics SetupAPI evidence"
assert_file_contains "$VULKAN_PROBE" "vkCreateInstance" "Vulkan probe instance creation"
assert_file_contains "$VULKAN_PROBE" "vkEnumeratePhysicalDevices" "Vulkan probe physical-device enumeration"
assert_file_contains "$VULKAN_PROBE" "[IO.File]::AppendAllText" "Vulkan probe durable phase beacon"
assert_file_contains "$VULKAN_PROBE" "VK_DRIVER_FILES" "Vulkan probe deterministic ICD selection"
assert_file_contains "$VULKAN_PROBE" "VK_LOADER_DEBUG" "Vulkan probe loader diagnostics"
assert_file_contains "$VULKAN_PROBE" "direct_icd_load_begin" "Vulkan probe direct ICD load boundary"
assert_file_contains "$VULKAN_PROBE" "create_instance_begin" "Vulkan probe create-instance boundary"
assert_file_contains "$VULKAN_PROBE" "enumerate_physical_devices_begin" "Vulkan probe physical-device boundary"
assert_file_contains "$DIAGNOSTICS_RUNNER" "bvgpu-diagnostics-latest.log" "diagnostics runner diagnostics log"
assert_file_contains "$DIAGNOSTICS_RUNNER" "bvgpu-vulkan-probe.log" "diagnostics runner Vulkan log"
assert_file_contains "$DIAGNOSTICS_RUNNER" "vulkan_exit_code" "diagnostics runner Vulkan exit status"
assert_file_contains "$DIAGNOSTICS_RUNNER" 'reg delete "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"' "diagnostics runner one-shot Run cleanup"
assert_file_contains "$DIAGNOSTICS_RUNNER" "bvgpu-diagnostics-running" "diagnostics runner concurrency guard"
assert_file_contains "$DIAGNOSTICS_RUNNER" "Programs\Startup\BridgeVMGpuDiagnostics.cmd" "diagnostics runner Startup cleanup"
assert_file_contains "$DIAGNOSTICS_RUNNER" "bvgpu-runner-entry.log" "diagnostics runner entry marker"
assert_file_contains "$DIAGNOSTICS_RUNNER" "viogpu3d-firstboot-pending.flag" "firstboot service pending gate"
assert_file_contains "$DIAGNOSTICS_RUNNER" "call C:\BridgeVM\bvgpu-firstboot.cmd" "firstboot service handoff"
assert_file_contains "$DIAGNOSTICS_RUNNER" "if not exist C:\BridgeVM\stage1.flag exit /b 1" "firstboot service success gate"
assert_file_contains "$DIAGNOSTICS_RUNNER" "sc.exe delete BridgeVMGpuDiagnosticsProbe6" "diagnostics runner service cleanup"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "StartServiceCtrlDispatcherW" "native diagnostics service dispatcher"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "bvgpu-native-service-entry.log" "native diagnostics service entry marker"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "WTSQueryUserToken" "native diagnostics active-console token"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "WTSEnumerateSessionsW" "native diagnostics headless-session enumeration"
assert_file_contains "$D3DKMT_PROBE_SOURCE" "D3DKMTEnumAdapters2" "D3DKMT adapter enumeration probe"
assert_file_contains "$D3DKMT_PROBE_SOURCE" "KMTQAITYPE_UMDRIVERPRIVATE" "D3DKMT viogpu private query probe"
assert_file_contains "$DIAGNOSTICS_RUNNER" "bvgpu-d3dkmt-probe.exe" "diagnostics runner D3DKMT probe"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "CreateProcessAsUserW" "native diagnostics interactive launch"
assert_file_contains "$DIAGNOSTICS_SERVICE_SOURCE" "firstboot-pending-session0" "native firstboot Session-0 launch"
assert_file_contains "$DIAGNOSTICS_RUNNER" "VN_DEBUG=init,result" "Vulkan renderer debug logging"
assert_file_contains "$DIAGNOSTICS_STARTUP" "bvgpu-diagnostics-run.cmd" "diagnostics Startup handoff"
assert_file_contains "$INJECTOR" "/v !BridgeVMGpu3DStage1" "injector initial RunOnce"
assert_file_contains "$INJECTOR" "copy /y %DRV%\..\bvgpu-diagnostics.ps1" "injector diagnostics copy"
assert_file_contains "$INJECTOR" "copy /y %DRV%\..\bvgpu-vulkan-probe.ps1" "injector Vulkan probe copy"
assert_file_contains "$BUILD_INJECTOR" 'cp "$ASSETS/bvgpu-diagnostics.ps1" "$DST_VOL/bvgpu-diagnostics.ps1"' "builder diagnostics staging"
assert_file_contains "$BUILD_INJECTOR" 'cp "$ASSETS/bvgpu-vulkan-probe.ps1" "$DST_VOL/bvgpu-vulkan-probe.ps1"' "builder Vulkan probe staging"
assert_file_contains "$BUILD_INJECTOR" 'DIAGNOSTICS_ONLY="${DIAGNOSTICS_ONLY:-0}"' "builder diagnostics-only switch"
assert_file_contains "$BUILD_INJECTOR" 'SKIP_OFFLINE_DISM="${SKIP_OFFLINE_DISM:-0}"' "builder live-activation switch"
assert_file_contains "$BUILD_INJECTOR" 'QUARANTINE_VIOGPU3D="${QUARANTINE_VIOGPU3D:-0}"' "builder crashing-driver quarantine switch"
assert_file_contains "$BUILD_INJECTOR" 'bridgevm-diagnostics-only.txt' "builder diagnostics-only marker"
assert_file_contains "$BUILD_INJECTOR" 'bridgevm-skip-offline-dism.txt' "builder skip-offline-DISM marker"
assert_file_contains "$BUILD_INJECTOR" 'bridgevm-quarantine-viogpu3d.txt' "builder crashing-driver quarantine marker"
assert_file_contains "$BUILD_INJECTOR" 'bvgpu-diagnostics-startup.cmd' "builder diagnostics Startup staging"
assert_file_contains "$INJECTOR" "BVINJECT DIAGNOSTICS-ONLY PLANT" "injector diagnostics-only branch"
assert_file_contains "$INJECTOR" "BVINJECT DISM SKIPPED FOR LIVE ACTIVATION" "injector live-activation DISM bypass"
assert_file_contains "$INJECTOR" 'CurrentVersion\Run" /v BridgeVMGpuDiagnostics' "injector diagnostics Run handoff"
assert_file_contains "$INJECTOR" "Programs\Startup\BridgeVMGpuDiagnostics.cmd" "injector diagnostics Startup handoff"
assert_file_contains "$INJECTOR" 'Services\BridgeVMGpuDiagnosticsProbe6" /v ImagePath' "injector diagnostics boot-service handoff"
assert_file_contains "$INJECTOR" 'bvgpu-diagnostics-service.exe" /f' "injector native diagnostics service command"
assert_file_contains "$INJECTOR" 'bvgpu-native-service-entry.log' "injector diagnostics service entry marker"
assert_file_contains "$INJECTOR" 'viogpu3d-firstboot-pending.flag' "injector firstboot pending marker"
assert_file_contains "$INJECTOR" 'BridgeVM one-shot viogpu3d activation' "injector firstboot native service"
assert_file_contains "$INJECTOR" "BVINJECT VIOGPU3D BOOT QUARANTINE" "injector crashing-driver quarantine branch"
assert_file_contains "$INJECTOR" 'Services\VioGpu3D" /v Start /t REG_DWORD /d 0x4' "injector offline viogpu3d disable"
assert_file_contains "$BUILD_INJECTOR" 'zig cc -target aarch64-windows-gnu' "builder native ARM64 diagnostics service"
assert_file_contains "$BUILD_INJECTOR" 'NEEDS_GPU_FIRSTBOOT' "builder normal viogpu3d service gate"
assert_file_contains "$INJECTOR" "goto :end" "injector diagnostics mutation barrier"
assert_file_contains "$INJECTOR" "BCD testsigning" "injector testsigning explanation"
assert_file_contains "$INJECTOR" "(on first boot) only lets the kernel LOAD" "injector live testsigning timing"
assert_file_contains "$INJECTOR" "PCI\VEN_1AF4&DEV_1050 or PCI\VEN_1AF4&DEV_10F7" "injector supported GPU HWIDs"

assert_file_not_contains "$FIRSTBOOT" "/v !BridgeVMGpu3D /t REG_SZ" "firstboot generic RunOnce reuse"
assert_file_not_contains "$INJECTOR" "/v !BridgeVMGpu3D /t REG_SZ" "injector generic RunOnce reuse"
assert_file_not_contains "$INJECTOR" "(above) only lets the kernel LOAD" "injector stale offline testsigning wording"
assert_file_not_contains "$FIRSTBOOT" "set NEXT_STAGE_2=" "firstboot stage2 RunOnce removal"
assert_file_not_contains "$FIRSTBOOT" "set NEXT_STAGE_3=" "firstboot stage3 RunOnce removal"
assert_file_not_contains "$FIRSTBOOT" "^|" "firstboot quoted PowerShell pipe escaping"
assert_file_contains "$FIRSTBOOT" "DEV_(1050|10F7)" "firstboot PowerShell regex alternation"
assert_file_contains "$FIRSTBOOT" "PrefetchParameters\" /v BootId" "firstboot registry boot identity"
assert_file_not_contains "$FIRSTBOOT" "Get-CimInstance Win32_OperatingSystem" "firstboot avoids WMI boot identity hang"
assert_file_contains "$FIRSTBOOT" 'if /i "%CURRENT_BOOT_ID%"=="%PREVIOUS_BOOT_ID%"' "firstboot same-boot rejection"

stage1="$(block_between_labels "$FIRSTBOOT" ":stage1" ":stage2")"
assert_block_contains "$stage1" "bcdedit /set {current} testsigning on" "stage1"
assert_block_contains "$stage1" "certutil -f -addstore Root" "stage1"
assert_block_contains "$stage1" "certutil -f -addstore TrustedPublisher" "stage1"
assert_block_contains "$stage1" "schtasks /Create" "stage1"
assert_block_contains "$stage1" "/SC ONSTART /DELAY 0001:00 /RU SYSTEM /RL HIGHEST" "stage1"
assert_block_contains "$stage1" "call :write_boot_identity C:\BridgeVM\stage1.boot" "stage1"
assert_block_contains "$stage1" "echo done > C:\BridgeVM\stage1.flag" "stage1"
assert_block_before "$stage1" "schtasks /Create" "call :write_boot_identity C:\BridgeVM\stage1.boot" "stage1 task handoff"
assert_block_before "$stage1" "call :write_boot_identity C:\BridgeVM\stage1.boot" "echo done > C:\BridgeVM\stage1.flag" "stage1 boot receipt"
assert_block_contains "$stage1" "if errorlevel 1 goto :fail" "stage1 failure gate"
assert_block_contains "$stage1" "shutdown /r /t 5 /c \"BridgeVM viogpu3d stage1\"" "stage1"
assert_block_not_contains "$stage1" "pnputil /add-driver" "stage1"
assert_block_not_contains "$stage1" "reg add \"%RO%\"" "stage1 RunOnce mutation"

stage2="$(block_between_labels "$FIRSTBOOT" ":stage2" ":stage3")"
assert_block_contains "$stage2" "call :require_new_boot C:\BridgeVM\stage1.boot" "stage2 reboot gate"
assert_block_contains "$stage2" "pnputil /add-driver" "stage2"
assert_block_contains "$stage2" "if errorlevel 260 goto :fail" "stage2 pnputil hard failure gate"
assert_block_contains "$stage2" "if errorlevel 259 (" "stage2 pnputil already-current status"
assert_block_contains "$stage2" "goto :stage2_scan" "stage2 pnputil already-current continuation"
assert_block_contains "$stage2" "sc.exe config VioGpu3D start= demand" "stage2 quarantine release"
assert_block_contains "$stage2" "pnputil /scan-devices" "stage2"
assert_block_before "$stage2" "sc.exe config VioGpu3D start= demand" "pnputil /scan-devices" "stage2 release quarantine before scan"
assert_block_contains "$stage2" "call :write_boot_identity C:\BridgeVM\stage2.boot" "stage2 boot receipt"
assert_block_contains "$stage2" "echo done > C:\BridgeVM\stage2.flag" "stage2"
assert_block_before "$stage2" "call :require_new_boot C:\BridgeVM\stage1.boot" "pnputil /add-driver" "stage2 reboot-before-install"
assert_block_before "$stage2" "call :write_boot_identity C:\BridgeVM\stage2.boot" "echo done > C:\BridgeVM\stage2.flag" "stage2 boot receipt"
assert_block_contains "$stage2" "if errorlevel 1 goto :fail" "stage2 failure gate"
assert_block_contains "$stage2" "shutdown /r /t 5 /c \"BridgeVM viogpu3d stage2\"" "stage2"
assert_block_not_contains "$stage2" "bcdedit /set {current} testsigning on" "stage2"
assert_block_not_contains "$stage2" "reg add \"%RO%\"" "stage2 RunOnce mutation"

stage3="$(block_between_labels "$FIRSTBOOT" ":stage3" ":fail")"
assert_block_contains "$stage3" "call :require_new_boot C:\BridgeVM\stage2.boot" "stage3 reboot gate"
assert_block_contains "$stage3" "bvgpu-diagnostics.ps1" "stage3 diagnostics"
assert_block_contains "$stage3" "bvgpu-vulkan-probe.ps1" "stage3 Vulkan probe"
assert_block_contains "$stage3" "Get-PnpDevice -PresentOnly" "stage3"
assert_block_contains "$stage3" "Get-CimInstance Win32_PnPSignedDriver" "stage3"
assert_block_contains "$stage3" "Get-FileHash -Algorithm SHA256 -LiteralPath \$expectedInf" "stage3 injected INF hash"
assert_block_contains "$stage3" "Get-FileHash -Algorithm SHA256 -LiteralPath \$boundInf" "stage3 bound INF hash"
assert_block_contains "$stage3" 'if ($boundHash -ne $expectedHash)' "stage3 exact INF identity"
assert_block_contains "$stage3" "set VULKAN_STATUS=%ERRORLEVEL%" "stage3 Vulkan status capture"
assert_block_contains "$stage3" 'if "%VULKAN_STATUS%"=="0" goto :vulkan_ok' "stage3 Vulkan success gate"
assert_block_contains "$stage3" "schtasks /Delete /TN \"%TASK_NAME%\" /F" "stage3 task cleanup"
assert_block_before "$stage3" 'if ($boundHash -ne $expectedHash)' "schtasks /Delete" "stage3 verify before cleanup"
assert_block_before "$stage3" "bvgpu-vulkan-probe.ps1" "bvgpu-diagnostics.ps1" "stage3 Vulkan before slow diagnostics"
assert_block_before "$stage3" 'if "%VULKAN_STATUS%"=="0" goto :vulkan_ok' "schtasks /Delete" "stage3 Vulkan gate before cleanup"
assert_block_before "$stage3" "bvgpu-diagnostics.ps1" "Get-PnpDevice -PresentOnly" "stage3 diagnose before status gate"
assert_block_contains "$stage3" "if errorlevel 1 goto :fail" "stage3 failure gate"
assert_block_contains "$stage3" "echo [stage3] done" "stage3"
assert_block_not_contains "$stage3" "shutdown /r" "stage3"
assert_block_not_contains "$stage3" "reg add \"%RO%\"" "stage3"

shutdown_count="$(grep -Fc 'shutdown /r /t 5 /c "BridgeVM viogpu3d stage' "$FIRSTBOOT")"
[[ "$shutdown_count" == "2" ]] || fail "expected exactly two staged reboots, got $shutdown_count"

task_create_count="$(grep -Fc 'schtasks /Create /TN "%TASK_NAME%"' "$FIRSTBOOT")"
[[ "$task_create_count" == "1" ]] || fail "expected exactly one persistent task creation, got $task_create_count"
task_delete_count="$(grep -Fc 'schtasks /Delete /TN "%TASK_NAME%"' "$FIRSTBOOT")"
[[ "$task_delete_count" == "1" ]] || fail "expected exactly one success-only task deletion, got $task_delete_count"

assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage1.flag" "injector stale stage1 cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage2.flag" "injector stale stage2 cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage1.boot" "injector stale stage1 boot cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage2.boot" "injector stale stage2 boot cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\gpu-rebooted.flag" "injector legacy marker cleanup"

echo "PASS: viogpu3d firstboot asset smoke"
