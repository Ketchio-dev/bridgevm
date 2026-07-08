#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-virtio-gpu-trace-report.XXXXXX")"
TRACE="$STORE/virtio-gpu.jsonl"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
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

cat >"$TRACE" <<'JSONL'
{"seq":1,"event":"device_init","width":1280,"height":720,"backend_3d":true}
{"seq":2,"event":"common_read","field":"device_features","device_features_sel":0,"value":27}
{"seq":3,"event":"common_read","field":"device_features","device_features_sel":1,"value":1}
{"seq":4,"event":"driver_features","select":0,"accepted":25}
{"seq":5,"event":"driver_features","select":1,"accepted":1}
{"seq":6,"event":"queue_notify","queue":0,"valid":true}
{"seq":7,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":4,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":8,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":4,"capset_version":1}
{"seq":9,"event":"command","name":"RESOURCE_CREATE_BLOB","response_name":"OK_NODATA"}
{"seq":10,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":4}
{"seq":11,"event":"command","name":"SUBMIT_3D","response_name":"OK_NODATA","fenced":true,"submit_size":16}
{"seq":12,"event":"fence_create","ctx_id":1,"ring_idx":0,"fence_id":9,"backend_accepted":true,"outcome":"parked"}
{"seq":13,"event":"fence_deliver","ctx_id":1,"ring_idx":0,"fence_id":9,"used_len":24}
JSONL

output="$(
  cargo run -q -p bridgevm-cli -- \
    hvf virtio-gpu-trace-report \
    --trace "$TRACE" \
    --protocol auto \
    --require-p3-gate 2>&1
)" || fail "virtio-gpu trace report failed: $output"

assert_contains "$output" "BridgeVM HVF virtio-gpu trace report" "virtio-gpu trace report"
assert_contains "$output" "Requested protocol: auto" "virtio-gpu trace report"
assert_contains "$output" "Selected protocol: venus" "virtio-gpu trace report"
assert_contains "$output" "3D features accepted: true" "virtio-gpu trace report"
assert_contains "$output" "GET_CAPSET OK: true" "virtio-gpu trace report"
assert_contains "$output" "GET_CAPSET_INFO VENUS id 4: true" "virtio-gpu trace report"
assert_contains "$output" "GET_CAPSET VENUS id 4: true" "virtio-gpu trace report"
assert_contains "$output" "RESOURCE_CREATE_BLOB OK: true" "virtio-gpu trace report"
assert_contains "$output" "CTX_CREATE OK: true" "virtio-gpu trace report"
assert_contains "$output" "CTX_CREATE VENUS context_init: true" "virtio-gpu trace report"
assert_contains "$output" "SUBMIT_3D OK: true" "virtio-gpu trace report"
assert_contains "$output" "SUBMIT_3D non-empty: true" "virtio-gpu trace report"
assert_contains "$output" "Backend-parked fence observed: true" "virtio-gpu trace report"
assert_contains "$output" "Fence deliver observed: true" "virtio-gpu trace report"
assert_contains "$output" "P3 Windows 3D trace gate: PASS" "virtio-gpu trace report"
assert_contains "$output" "Blockers: none" "virtio-gpu trace report"

echo "PASS: HVF virtio-gpu trace report CLI smoke ($STORE)"
