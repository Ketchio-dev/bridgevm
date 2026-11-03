#!/usr/bin/env bash
# Prepare a test image for the A8 window-resize check (verify-a8-window-resize.sh).
# Steps:
# 1. Boot an APFS clone with the 2D virtio-gpu device (`--virtio-gpu`, PCI 1AF4:1050).
# 2. Install the Microsoft attestation-signed Red Hat virtio-win viogpudo from DRIVER_DIR
#    (the pinned upstream archive's Win11/ARM64 folder) with pnputil.
# 3. Report Secure Boot and the display adapters, then shut down.
# The prepared disk and vars stay in $OUT/work. The source image is only cloned.
# In the product the app's display-driver action installs the same driver; the guest
# downloads it itself.
# REMOVE_PS1 (optional, development images only): removes an older test-signed display
# package first, for example scripts/win-assets/bvgpu-remove-test-3d-package.ps1.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:?installed Windows disk (cloned, never modified)}
VARS=${VARS:?its UEFI variables (copied)}
DRIVER_DIR=${DRIVER_DIR:?folder holding viogpudo.inf, .cat and .sys}
PROBE=${PROBE:?prebuilt release probe}
OUT=${OUT:?output directory}
REMOVE_PS1=${REMOVE_PS1:-}
GUEST_EPOCH=${GUEST_EPOCH:-}   # optional: boot a date-bound test image at a fixed date
BOOT_TIMEOUT=${BOOT_TIMEOUT:-600}
STEP_TIMEOUT=${STEP_TIMEOUT:-120}
REPORT_PS1=scripts/win-assets/bvgpu-display-report.ps1

mkdir -p "$OUT/work/bin"
WORK=$OUT/work
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"
chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"   # staged sources are read-only
cp "$PROBE" "$WORK/bin/hvf_gic_boot_probe"
codesign --sign - --force --entitlements apps/macos/HvfRunner.entitlements "$WORK/bin/hvf_gic_boot_probe"
CTL=$OUT/agent.ctl; : > "$CTL"
EVIDENCE=$OUT/hvf; RUN_LOG=$EVIDENCE/run.log
SHARE=$OUT/share-host; mkdir -p "$SHARE"
cp "$DRIVER_DIR"/viogpudo.inf "$DRIVER_DIR"/viogpudo.cat "$DRIVER_DIR"/viogpudo.sys "$REPORT_PS1" "$SHARE/"
[[ -z "$REMOVE_PS1" ]] || cp "$REMOVE_PS1" "$SHARE/"

count() { tr -d '\r' 2>/dev/null < "$RUN_LOG" | grep -cE "$1" || true; }
wait_for() {   # regex, count, seconds
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(count "$1"); (( ${n:-0} >= $2 )) && return 0
    sleep 0.3
  done
  return 1
}
send() {
  local before; before=$(count '^BVAGENT END '); before=${before:-0}
  printf '%s\n' "$1" >> "$CTL"
  wait_for '^BVAGENT END ' $((before + 1)) "$STEP_TIMEOUT" || fail "no reply for: ${1:0:90}"
}
shared() { wait_for "^BVAGENT SHARE host->guest $(basename "$1" | sed 's/\./\\./g')" 1 "$STEP_TIMEOUT" || fail "$(basename "$1") not shared"; }
pnp() { send "powershell -NoProfile -Command \"Get-PnpDevice -Class Display | ForEach-Object { 'BVPNP $1 {0} status={1} problem={2} id={3}' -f \$_.FriendlyName,\$_.Status,\$_.Problem,\$_.InstanceId }\""; }
report() { send "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BVRS\\$(basename "$REPORT_PS1")"; }
# Not grep -q: under pipefail its early exit kills tr with SIGPIPE and the pipeline fails.
logged() { tr -d '\r' < "$RUN_LOG" | grep -E "$1" > /dev/null; }

LANE_PROBE=$WORK/bin/hvf_gic_boot_probe
lane_vm_running() { pgrep -f "^$LANE_PROBE( |\$)" > /dev/null; }
# Stops this lane's VM: TERM the launcher (it stops its own probe), then TERM and
# finally KILL the probe copy under $WORK, whose path is unique to this run.
stop_vm() {
  kill -TERM "$LAUNCHER" 2>/dev/null || true
  for _ in $(seq 1 60); do
    kill -0 "$LAUNCHER" 2>/dev/null || lane_vm_running || break
    sleep 0.5
  done
  if lane_vm_running; then pkill -TERM -f "^$LANE_PROBE( |\$)" || true; sleep 5; fi
  if lane_vm_running || kill -0 "$LAUNCHER" 2>/dev/null; then
    pkill -KILL -f "^$LANE_PROBE( |\$)" || true
    kill -KILL "$LAUNCHER" 2>/dev/null || true
    sleep 1
  fi
  wait "$LAUNCHER" 2>/dev/null || true
  ! lane_vm_running
}

ENV=(BRIDGEVM_PREBUILT_PROBE="$LANE_PROBE")
[[ -z "$GUEST_EPOCH" ]] || ENV+=(BRIDGEVM_GUEST_RTC_EPOCH="$GUEST_EPOCH")
env "${ENV[@]}" scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$EVIDENCE" --release --skip-build --no-guest-disk-harvest \
  --watchdog-ms $((BOOT_TIMEOUT * 4000)) --ram-mib 6144 --smp-cpus 4 --enable-xhci \
  --virtio-gpu --agent-service-control "$CTL" \
  --agent-share-host "$SHARE" --agent-share-guest 'C:\BVRS' > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'stop_vm || echo "FAIL: this lane VM is still running" >&2' EXIT
wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" || fail "agent service timeout"
shared viogpudo.sys; shared "$REPORT_PS1"

send 'powershell -NoProfile -Command "Write-Output (\"secure_boot=\" + (Confirm-SecureBootUEFI))"'
pnp before; report
if [[ -n "$REMOVE_PS1" ]]; then
  shared "$REMOVE_PS1"
  send "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BVRS\\$(basename "$REMOVE_PS1")"
fi
send 'pnputil /add-driver C:\BVRS\viogpudo.inf /install'
send 'pnputil /scan-devices'
sleep 5
pnp after; report
if logged 'BVPNP after .*VirtIO.*problem=CM_PROB_NEED_RESTART'; then
  starts=$(count '^BVAGENT SERVICE start')
  printf '%s\n' 'shutdown /r /t 2' >> "$CTL"
  wait_for '^BVAGENT SERVICE start' $((starts + 1)) "$BOOT_TIMEOUT" || fail "agent did not return after the restart"
  pnp rebooted; report
fi
printf '%s\n' 'shutdown /s /t 2' >> "$CTL"
for _ in $(seq 1 240); do kill -0 "$LAUNCHER" 2>/dev/null || break; sleep 0.5; done
if kill -0 "$LAUNCHER" 2>/dev/null; then
  fail "Windows did not power off within 120 s; the prepared pair is incomplete"
fi
trap - EXIT
stop_vm || fail "this lane VM is still running"
rm -rf "${WORK:?}/bin"
tr -d '\r' < "$RUN_LOG" | grep -aE '^(secure_boot=|BVPNP |BVDISP |BVCLEAN |BVERR)' | head -60
logged 'BVPNP (after|rebooted) .*VirtIO GPU DOD.*status=OK' || fail "viogpudo is not running on the virtio-gpu device"
echo "prepared: $WORK/disk.raw $WORK/vars.fd"
