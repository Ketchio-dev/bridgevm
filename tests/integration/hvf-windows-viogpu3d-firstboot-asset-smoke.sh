#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIRSTBOOT="scripts/win-assets/bvgpu-firstboot.cmd"
INJECTOR="scripts/win-assets/bvinject.cmd"

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
assert_file_contains "$INJECTOR" "/v !BridgeVMGpu3DStage1" "injector initial RunOnce"
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
assert_file_contains "$FIRSTBOOT" "LastBootUpTime.ToFileTimeUtc()" "firstboot stable boot identity"
assert_file_contains "$FIRSTBOOT" 'if ($current -eq $previous)' "firstboot same-boot rejection"

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
assert_block_contains "$stage2" "pnputil /scan-devices" "stage2"
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
assert_block_contains "$stage3" "Get-PnpDevice -PresentOnly" "stage3"
assert_block_contains "$stage3" "Get-CimInstance Win32_PnPSignedDriver" "stage3"
assert_block_contains "$stage3" "Get-FileHash -Algorithm SHA256 -LiteralPath \$expectedInf" "stage3 injected INF hash"
assert_block_contains "$stage3" "Get-FileHash -Algorithm SHA256 -LiteralPath \$boundInf" "stage3 bound INF hash"
assert_block_contains "$stage3" 'if ($boundHash -ne $expectedHash)' "stage3 exact INF identity"
assert_block_contains "$stage3" "schtasks /Delete /TN \"%TASK_NAME%\" /F" "stage3 task cleanup"
assert_block_before "$stage3" 'if ($boundHash -ne $expectedHash)' "schtasks /Delete" "stage3 verify before cleanup"
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
