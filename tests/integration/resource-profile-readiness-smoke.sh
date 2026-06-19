#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-resource-profile.XXXXXX")"
VM_PERF="resource-performance"
VM_MANUAL="resource-manual"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""
PRESERVE_STORE=1

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

lightvm_runner() {
  cargo run --quiet -p lightvm-runner -- --store "$STORE" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
}

make_ready_fast_vm() {
  local vm="$1"
  local bundle="$STORE/vms/$vm.vmbridge"
  local manifest="$bundle/manifest.yaml"
  local disk="$bundle/disks/root.raw"
  local kernel="$bundle/boot/vmlinuz"

  bridgevm create "$vm" \
    --os ubuntu \
    --arch arm64 \
    --mode fast \
    --boot-mode linux-kernel \
    --kernel-path boot/vmlinuz >/dev/null
  perl -0pi -e 's#path: disks/root\.qcow2\n    size: 80GiB\n    format: qcow2#path: disks/root.raw\n    size: 64MiB\n    format: raw#' \
    "$manifest"
  mkdir -p "$(dirname "$disk")" "$(dirname "$kernel")"
  : >"$disk"
  : >"$kernel"
}

set_resources() {
  local vm="$1"
  local profile="$2"
  local memory="$3"
  local cpu="$4"
  local manifest="$STORE/vms/$vm.vmbridge/manifest.yaml"

  perl -0pi -e "s/profile: automatic/profile: $profile/; s/memory: auto/memory: $memory/; s/cpu: auto/cpu: $cpu/" \
    "$manifest"
}

assert_launch_spec_resources() {
  local label="$1"
  local path="$2"
  local memory="$3"
  local cpu="$4"
  local fps="$5"
  local rationale="$6"
  local balloon="$7"

  python3 - "$path" "$memory" "$cpu" "$fps" "$rationale" "$balloon" <<'PY' \
    || fail "$label launch spec resources did not match"
import json
import sys

path, memory, cpu, fps, rationale, balloon_text = sys.argv[1:7]
with open(path, "r", encoding="utf-8") as handle:
    spec = json.load(handle)
resources = spec.get("resources") or {}
readiness = spec.get("readiness") or {}
checks = [
    readiness.get("ready") is True,
    resources.get("memory") == memory,
    resources.get("cpu") == cpu,
    resources.get("display_fps_cap") == fps,
    resources.get("rationale") == rationale,
    resources.get("balloon_device") is (balloon_text == "true"),
]
sys.exit(0 if all(checks) else 1)
PY
}

assert_handoff_resources() {
  local label="$1"
  local output="$2"
  local memory="$3"
  local cpu="$4"
  local fps="$5"
  local balloon="$6"

  assert_contains "$output" '"backend": "apple-virtualization-framework"' "$label"
  assert_contains "$output" '"ready": true' "$label"
  assert_contains "$output" '"resources": {' "$label"
  assert_contains "$output" "\"memory\": \"$memory\"" "$label"
  assert_contains "$output" "\"cpu\": \"$cpu\"" "$label"
  assert_contains "$output" "\"display_fps_cap\": \"$fps\"" "$label"
  assert_contains "$output" "\"balloon_device\": $balloon" "$label"
}

assert_apple_vz_config_plan() {
  local label="$1"
  local handoff_json="$2"
  local vm="$3"
  local memory="$4"
  local cpu="$5"

  local output
  output="$(
    cd apps/macos
    swift run --quiet AppleVzRunner --handoff-json "$handoff_json" --validate-only --print-config-plan
  )"
  assert_contains "$output" "AppleVzRunner handoff ready" "$label"
  assert_contains "$output" "VM: $vm" "$label"
  assert_contains "$output" "Boot mode: linux-kernel" "$label"
  assert_contains "$output" "Memory MiB: $memory" "$label"
  assert_contains "$output" "CPU count: $cpu" "$label"
  assert_contains "$output" "Configuration plan:" "$label"
  assert_contains "$output" "Disk attachment: disk-image-raw" "$label"
}

trap stop_daemon EXIT

make_ready_fast_vm "$VM_PERF"
set_resources "$VM_PERF" "performance" "auto" "auto"
PERF_BUNDLE="$STORE/vms/$VM_PERF.vmbridge"
PERF_LAUNCH_SPEC="$PERF_BUNDLE/metadata/apple-vz-launch.json"
PERF_HANDOFF_JSON="$STORE/performance-handoff.json"
perf_run="$(bridgevm run "$VM_PERF")"
assert_contains "$perf_run" "Engine: lightvm" "performance run"
assert_contains "$perf_run" "Launch ready: true" "performance run"
assert_contains "$perf_run" "Launch spec: $PERF_LAUNCH_SPEC" "performance run"
assert_launch_spec_resources \
  "performance profile" \
  "$PERF_LAUNCH_SPEC" \
  "6144" \
  "4" \
  "60" \
  "Foreground performance profile." \
  "false"
lightvm_runner --launch-spec "$PERF_LAUNCH_SPEC" --require-ready --print-handoff \
  >"$PERF_HANDOFF_JSON"
perf_handoff="$(cat "$PERF_HANDOFF_JSON")"
assert_handoff_resources "performance handoff" "$perf_handoff" "6144" "4" "60" "false"
assert_apple_vz_config_plan "performance AppleVzRunner plan" "$PERF_HANDOFF_JSON" "$VM_PERF" "6144" "4"

make_ready_fast_vm "$VM_MANUAL"
set_resources "$VM_MANUAL" "performance" "8192" "6"
MANUAL_BUNDLE="$STORE/vms/$VM_MANUAL.vmbridge"
MANUAL_LAUNCH_SPEC="$MANUAL_BUNDLE/metadata/apple-vz-launch.json"
MANUAL_HANDOFF_JSON="$STORE/manual-handoff.json"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon exited before socket became ready"
  fi
  sleep 0.05
done
[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

manual_run="$(bridgevm_socket run "$VM_MANUAL")"
assert_contains "$manual_run" "Engine: lightvm" "manual socket run"
assert_contains "$manual_run" "Launch ready: true" "manual socket run"
assert_contains "$manual_run" "Launch spec: $MANUAL_LAUNCH_SPEC" "manual socket run"
assert_launch_spec_resources \
  "manual override" \
  "$MANUAL_LAUNCH_SPEC" \
  "8192" \
  "6" \
  "60" \
  "Foreground performance profile." \
  "false"
lightvm_runner --launch-spec "$MANUAL_LAUNCH_SPEC" --require-ready --print-handoff \
  >"$MANUAL_HANDOFF_JSON"
manual_handoff="$(cat "$MANUAL_HANDOFF_JSON")"
assert_handoff_resources "manual handoff" "$manual_handoff" "8192" "6" "60" "false"
assert_apple_vz_config_plan "manual AppleVzRunner plan" "$MANUAL_HANDOFF_JSON" "$VM_MANUAL" "8192" "6"

PRESERVE_STORE=0
echo "PASS: resource profile launch handoff smoke ($STORE)"
