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
assert_file_contains "$FIRSTBOOT" "set NEXT_STAGE_2=!BridgeVMGpu3DStage2" "firstboot stage2 RunOnce"
assert_file_contains "$FIRSTBOOT" "set NEXT_STAGE_3=!BridgeVMGpu3DStage3" "firstboot stage3 RunOnce"
assert_file_contains "$INJECTOR" "/v !BridgeVMGpu3DStage1" "injector initial RunOnce"

assert_file_not_contains "$FIRSTBOOT" "/v !BridgeVMGpu3D /t REG_SZ" "firstboot generic RunOnce reuse"
assert_file_not_contains "$INJECTOR" "/v !BridgeVMGpu3D /t REG_SZ" "injector generic RunOnce reuse"

stage1="$(block_between_labels "$FIRSTBOOT" ":stage1" ":stage2")"
assert_block_contains "$stage1" "bcdedit /set {current} testsigning on" "stage1"
assert_block_contains "$stage1" "certutil -f -addstore Root" "stage1"
assert_block_contains "$stage1" "certutil -f -addstore TrustedPublisher" "stage1"
assert_block_contains "$stage1" "echo done > C:\BridgeVM\stage1.flag" "stage1"
assert_block_contains "$stage1" "/v %NEXT_STAGE_2%" "stage1"
assert_block_before "$stage1" "/v %NEXT_STAGE_2%" "echo done > C:\BridgeVM\stage1.flag" "stage1 crash-safe handoff"
assert_block_contains "$stage1" "if errorlevel 1 goto :fail" "stage1 failure gate"
assert_block_contains "$stage1" "shutdown /r /t 5 /c \"BridgeVM viogpu3d stage1\"" "stage1"
assert_block_not_contains "$stage1" "pnputil /add-driver" "stage1"

stage2="$(block_between_labels "$FIRSTBOOT" ":stage2" ":stage3")"
assert_block_contains "$stage2" "pnputil /add-driver" "stage2"
assert_block_contains "$stage2" "pnputil /scan-devices" "stage2"
assert_block_contains "$stage2" "echo done > C:\BridgeVM\stage2.flag" "stage2"
assert_block_contains "$stage2" "/v %NEXT_STAGE_3%" "stage2"
assert_block_before "$stage2" "/v %NEXT_STAGE_3%" "echo done > C:\BridgeVM\stage2.flag" "stage2 crash-safe handoff"
assert_block_contains "$stage2" "if errorlevel 1 goto :fail" "stage2 failure gate"
assert_block_contains "$stage2" "shutdown /r /t 5 /c \"BridgeVM viogpu3d stage2\"" "stage2"
assert_block_not_contains "$stage2" "bcdedit /set {current} testsigning on" "stage2"

stage3="$(block_between_labels "$FIRSTBOOT" ":stage3" ":fail")"
assert_block_contains "$stage3" "Get-PnpDevice -PresentOnly" "stage3"
assert_block_contains "$stage3" "Get-CimInstance Win32_PnPSignedDriver" "stage3"
assert_block_contains "$stage3" "Select-String -Path \$inf -Pattern 'viogpu3d'" "stage3"
assert_block_contains "$stage3" "if errorlevel 1 goto :fail" "stage3 failure gate"
assert_block_contains "$stage3" "echo [stage3] done" "stage3"
assert_block_not_contains "$stage3" "shutdown /r" "stage3"
assert_block_not_contains "$stage3" "reg add \"%RO%\"" "stage3"

shutdown_count="$(grep -Fc 'shutdown /r /t 5 /c "BridgeVM viogpu3d stage' "$FIRSTBOOT")"
[[ "$shutdown_count" == "2" ]] || fail "expected exactly two staged reboots, got $shutdown_count"

assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage1.flag" "injector stale stage1 cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\stage2.flag" "injector stale stage2 cleanup"
assert_file_contains "$INJECTOR" "del /f /q %WIN%\BridgeVM\gpu-rebooted.flag" "injector legacy marker cleanup"

echo "PASS: viogpu3d firstboot asset smoke"
