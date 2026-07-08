#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-installed-p3-gpu-trace-report.XXXXXX")"
TRACE="$STORE/evidence/virtio-gpu.jsonl"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label file missing: $path"
  grep -Fq "$needle" "$path" || fail "$label missing '$needle' in $path"
}

mkdir -p "$(dirname "$TRACE")"
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

source scripts/run-hvf-windows-installed-boot-runner.sh

EVIDENCE_DIR="$STORE/evidence"
VIRTIO_GPU_3D="1"
VIRTIO_GPU_TRACE_JSONL="$TRACE"
GPU_TRACE_PROTOCOL="auto"
REQUIRE_GPU_TRACE_GATE="1"
RUN_STATUS="0"

write_virtio_gpu_trace_report

[[ "$RUN_STATUS" == "0" ]] || fail "passing trace gate changed RUN_STATUS to $RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "P3 Windows 3D trace gate: PASS" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "Requested protocol: auto" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "Selected protocol: venus" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "GET_CAPSET_INFO VENUS id 4: true" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "CTX_CREATE VENUS context_init: true" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "Backend-parked fence observed: true" "trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt" "required=1" "trace gate"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt" "protocol=auto" "trace gate"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt" "status=0" "trace gate"

EVIDENCE_DIR="$STORE/missing-evidence"
VIRTIO_GPU_TRACE_JSONL="$STORE/missing.jsonl"
RUN_STATUS="0"
mkdir -p "$EVIDENCE_DIR"

write_virtio_gpu_trace_report

[[ "$RUN_STATUS" == "1" ]] || fail "missing required trace did not promote RUN_STATUS; got $RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "Trace missing or empty" "missing trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-report.txt" "P3 Windows 3D trace gate: FAIL" "missing trace report"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt" "required=1" "missing trace gate"
assert_file_contains "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt" "status=1" "missing trace gate"

echo "PASS: installed Windows P3 GPU trace report smoke ($STORE)"
