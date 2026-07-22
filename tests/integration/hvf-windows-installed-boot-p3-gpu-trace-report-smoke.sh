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
source scripts/run-hvf-windows-installed-boot-runner.sh

EVIDENCE_DIR="$STORE/evidence"
VIRTIO_GPU_3D="1"
VIRTIO_GPU_TRACE_JSONL="$TRACE"
GPU_TRACE_PROTOCOL="auto"
REQUIRE_GPU_TRACE_GATE="1"
RUN_STATUS="0"

printf 'stale-success\n' > "$TRACE"
prepare_virtio_gpu_trace
[[ ! -s "$TRACE" ]] || fail "GPU trace preparation preserved stale evidence"

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

mkdir -p "$EVIDENCE_DIR/guest-logs"
cat > "$EVIDENCE_DIR/guest-logs/bvgpu-real-title-gate.log" <<'TITLELOG'
BVGPU-REAL-TITLE-PASS
elapsed_ms=30000
main_window_observed=true
module=vulkan_virtio.dll
TITLELOG
printf 'BVGPU-DRIVER-STATE-PASS\n' > "$EVIDENCE_DIR/guest-logs/viogpu3d-cleanup.log"
for i in $(seq 1 300); do
  printf '{"seq":%s,"event":"command","name":"RESOURCE_FLUSH"}\n' "$((1000 + i))" >> "$TRACE"
done
TITLE_MANIFESTS=("$ROOT/scripts/win-assets/bv-ppsspp-title.json")
TITLE_MANIFEST_COUNT=1
REQUIRE_TITLE_GATES="1"
printf '{"ppsspp-vulkan-arm64":"missing"}\n' > "$EVIDENCE_DIR/title-pre-run-state.json"
RUN_STATUS="0"
write_title_gate_report
[[ "$RUN_STATUS" == "0" ]] || fail "passing generic title gate changed RUN_STATUS to $RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/title-gates.txt" "Title: ppsspp-vulkan-arm64" "generic title report"
assert_file_contains "$EVIDENCE_DIR/title-gates.txt" "Gate: PASS" "generic title report"
assert_file_contains "$EVIDENCE_DIR/title-gates-gate.txt" "required=1" "generic title gate"
assert_file_contains "$EVIDENCE_DIR/title-gates-gate.txt" "status=0" "generic title gate"
assert_file_contains "$EVIDENCE_DIR/title-gates.json" '"passed": true' "generic title JSON"

current_title_sha256="$(shasum -a 256 "$EVIDENCE_DIR/guest-logs/bvgpu-real-title-gate.log" | awk '{print $1}')"
printf '{"ppsspp-vulkan-arm64":"%s"}\n' "$current_title_sha256" > "$EVIDENCE_DIR/title-pre-run-state.json"
RUN_STATUS="0"
write_title_gate_report
[[ "$RUN_STATUS" == "1" ]] || fail "stale generic title evidence did not fail the required gate"
assert_file_contains "$EVIDENCE_DIR/title-gates.txt" "guest log was not proven fresh" "stale generic title report"
assert_file_contains "$EVIDENCE_DIR/title-gates-gate.txt" "status=1" "stale generic title gate"

REQUIRE_REAL_TITLE_GATE="1"
PRE_RUN_REAL_TITLE_SHA256="missing"
RUN_STATUS="0"
write_real_title_gate_report
[[ "$RUN_STATUS" == "0" ]] || fail "passing real-title gate changed RUN_STATUS to $RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_marker_pass=1" "real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_fresh=1" "real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_pass=1" "real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "driver_state_pass=1" "real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "resource_flush_count=300" "real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "status=0" "real-title gate"

PRE_RUN_REAL_TITLE_SHA256="$(shasum -a 256 "$EVIDENCE_DIR/guest-logs/bvgpu-real-title-gate.log" | awk '{print $1}')"
RUN_STATUS="0"
write_real_title_gate_report
[[ "$RUN_STATUS" == "1" ]] || fail "stale PPSSPP pass did not fail required real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_marker_pass=1" "stale real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_fresh=0" "stale real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_pass=0" "stale real-title gate"

printf 'process crashed before gate\n' > "$EVIDENCE_DIR/guest-logs/bvgpu-real-title-gate.log"
PRE_RUN_REAL_TITLE_SHA256="missing"
RUN_STATUS="0"
write_real_title_gate_report
[[ "$RUN_STATUS" == "1" ]] || fail "missing PPSSPP pass did not fail required real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "guest_title_pass=0" "failing real-title gate"
assert_file_contains "$EVIDENCE_DIR/real-title-gate.txt" "status=1" "failing real-title gate"

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
