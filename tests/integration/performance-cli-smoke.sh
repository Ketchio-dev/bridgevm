#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-perf.XXXXXX")"
VM_NAME="legacy-linux"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
QEMU_VM_NAME="qcow2-linux"
QEMU_BUNDLE="$STORE/vms/$QEMU_VM_NAME.vmbridge"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  exit 1
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

extract_line_value() {
  local name="$1"
  local output="$2"
  printf '%s\n' "$output" | sed -n "s/^$name: //p" | head -n 1
}

assert_sample_contract() {
  local label="$1"
  local output="$2"
  local artifact_path="$3"
  local probe_path="$4"
  local vm_name="${5:-$VM_NAME}"
  local source_bundle="${6:-$BUNDLE}"
  local expected_bytes="${7:-4096}"
  local expected_iterations="${8:-1}"
  local expected_sync="${9:-false}"
  local expected_total_bytes=$((expected_bytes * expected_iterations))

  case "$output" in
    *"Performance sample for $vm_name"*) ;;
    *) fail "$label sample output did not name VM: $output" ;;
  esac
  case "$output" in
    *"Probe bytes: $expected_bytes"*) ;;
    *) fail "$label sample output did not report $expected_bytes probe bytes: $output" ;;
  esac
  case "$output" in
    *"Iterations: $expected_iterations"*) ;;
    *) fail "$label sample output did not report $expected_iterations iteration(s): $output" ;;
  esac
  case "$output" in
    *"Sync: $expected_sync"*) ;;
    *) fail "$label sample output did not report sync=$expected_sync: $output" ;;
  esac
  case "$output" in
    *"Measurement: host_artifact_write_latency_microseconds="*) ;;
    *) fail "$label sample output omitted host write latency measurement" ;;
  esac
  case "$output" in
    *"Measurement: host_artifact_write_total_bytes=$expected_total_bytes bytes (host.fs.write_probe)"*) ;;
    *) fail "$label sample output omitted host write total bytes measurement" ;;
  esac
  case "$output" in
    *"Measurement: host_artifact_write_iterations=$expected_iterations count (host.fs.write_probe)"*) ;;
    *) fail "$label sample output omitted host write iterations measurement" ;;
  esac
  case "$output" in
    *"Measurement: host_artifact_write_latency_p50_microseconds="*) ;;
    *) fail "$label sample output omitted host write p50 latency measurement" ;;
  esac

  [[ -f "$artifact_path" ]] || fail "$label sample metadata artifact missing: $artifact_path"
  [[ -f "$probe_path" ]] || fail "$label sample probe missing: $probe_path"

  local probe_size
  probe_size="$(wc -c <"$probe_path" | tr -d ' ')"
  [[ "$probe_size" == "$expected_bytes" ]] || fail "$label sample probe size was $probe_size bytes"

  grep -q "\"artifact_bytes\": $expected_bytes" "$artifact_path" \
    || fail "$label sample metadata did not record artifact_bytes"
  grep -q "\"source\": \"$source_bundle\"" "$artifact_path" \
    || fail "$label sample metadata did not record source bundle"
  grep -q "\"artifact\": \"$artifact_path\"" "$artifact_path" \
    || fail "$label sample metadata did not record artifact path"
  grep -q "\"probe\": \"$probe_path\"" "$artifact_path" \
    || fail "$label sample metadata did not record probe path"
  grep -q '"state": "stopped"' "$artifact_path" \
    || fail "$label sample metadata did not record stopped VM state"
  grep -q '"guest_tools": {' "$artifact_path" \
    || fail "$label sample metadata omitted guest_tools status"
  grep -q "\"iterations\": $expected_iterations" "$artifact_path" \
    || fail "$label sample metadata did not record iterations"
  grep -q "\"sync\": $expected_sync" "$artifact_path" \
    || fail "$label sample metadata did not record sync=$expected_sync"
  grep -q '"iteration": 1' "$artifact_path" \
    || fail "$label sample metadata did not record iteration result"
  grep -q "\"bytes\": $expected_bytes" "$artifact_path" \
    || fail "$label sample metadata did not record iteration bytes"
  grep -q '"name": "host_artifact_write_latency_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted write latency measurement"
  grep -q '"name": "host_artifact_write_latency_min_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted write min latency measurement"
  grep -q '"name": "host_artifact_write_latency_max_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted write max latency measurement"
  grep -q '"name": "host_artifact_write_latency_mean_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted write mean latency measurement"
  grep -q '"name": "host_artifact_write_latency_p50_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted write p50 latency measurement"
  grep -q '"name": "host_artifact_write_iterations"' "$artifact_path" \
    || fail "$label sample metadata omitted write iterations measurement"
  grep -q '"name": "host_artifact_write_total_bytes"' "$artifact_path" \
    || fail "$label sample metadata omitted write total bytes measurement"
  grep -q "\"value\": $expected_total_bytes" "$artifact_path" \
    || fail "$label sample metadata did not record expected total bytes"
  grep -q '"name": "bridgevm_guest_tools_status_inspect_latency_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted guest tools status latency measurement"
  grep -q '"name": "sample_generation_duration_microseconds"' "$artifact_path" \
    || fail "$label sample metadata omitted sample generation duration measurement"
  grep -q '"source": "host.fs.write_probe"' "$artifact_path" \
    || fail "$label sample metadata omitted host write measurement source"
  grep -q '"metadata_only": false' "$artifact_path" \
    || fail "$label sample metadata did not mark any measurement non-metadata"
  grep -q 'host-side sample; no guest benchmark workloads were executed' "$artifact_path" \
    || fail "$label sample metadata omitted no-guest-benchmark note"

  local probe_count
  probe_count="$(find "$(dirname "$artifact_path")" -maxdepth 1 -type f -name 'write-probe*.bin' | wc -l | tr -d ' ')"
  [[ "$probe_count" == "$expected_iterations" ]] \
    || fail "$label sample wrote $probe_count probe(s), expected $expected_iterations"
  local iteration_count
  iteration_count="$(grep -c '"write_latency_microseconds":' "$artifact_path")"
  [[ "$iteration_count" == "$expected_iterations" ]] \
    || fail "$label sample recorded $iteration_count iteration result(s), expected $expected_iterations"
}

assert_baseline_contract() {
  local label="$1"
  local output="$2"
  local artifact_path="$3"
  local vm_name="${4:-$VM_NAME}"
  local source_bundle="${5:-$BUNDLE}"

  case "$output" in
    *"Performance baseline for $vm_name"*) ;;
    *) fail "$label baseline output did not name VM: $output" ;;
  esac
  case "$output" in
    *"Metadata only: true"*) ;;
    *) fail "$label baseline output did not report metadata-only boundary: $output" ;;
  esac
  case "$output" in
    *"State: stopped"*) ;;
    *) fail "$label baseline output did not report stopped state: $output" ;;
  esac
  case "$output" in
    *"Guest metrics: unavailable"*) ;;
    *) fail "$label baseline output did not report unavailable guest metrics: $output" ;;
  esac
  case "$output" in
    *"Measurement: state_metadata_age_seconds="*) ;;
    *) fail "$label baseline output omitted VM state age measurement" ;;
  esac

  [[ -f "$artifact_path" ]] || fail "$label baseline metadata artifact missing: $artifact_path"

  grep -q "\"vm\": \"$vm_name\"" "$artifact_path" \
    || fail "$label baseline metadata did not record VM name"
  grep -q "\"source\": \"$source_bundle\"" "$artifact_path" \
    || fail "$label baseline metadata did not record source bundle"
  grep -q "\"artifact\": \"$artifact_path\"" "$artifact_path" \
    || fail "$label baseline metadata did not record artifact path"
  grep -q '"metadata_only": true' "$artifact_path" \
    || fail "$label baseline metadata did not mark baseline metadata-only"
  grep -q '"state": "stopped"' "$artifact_path" \
    || fail "$label baseline metadata did not record stopped VM state"
  grep -q '"guest_tools": {' "$artifact_path" \
    || fail "$label baseline metadata omitted guest_tools status"
  grep -q '"metrics": null' "$artifact_path" \
    || fail "$label baseline metadata did not record unavailable guest metrics"
  grep -q '"name": "state_metadata_age_seconds"' "$artifact_path" \
    || fail "$label baseline metadata omitted VM state age measurement"
  grep -q '"source": "state.updated_at_unix"' "$artifact_path" \
    || fail "$label baseline metadata omitted state measurement source"
  grep -q 'metadata-only baseline; no active benchmark workloads were executed' "$artifact_path" \
    || fail "$label baseline metadata omitted no-benchmark note"
}

assert_missing_vm_baseline_rejected() {
  local label="$1"
  local output_dir="$2"
  shift 2

  local stderr="$STORE/$label.stderr"
  if "$@" performance baseline missing-vm --output "$output_dir" >"$STORE/$label.stdout" 2>"$stderr"; then
    fail "$label unexpectedly accepted missing VM performance baseline"
  fi
  grep -q "VM not found" "$stderr" \
    || fail "$label stderr did not report missing VM: $(cat "$stderr")"
  [[ ! -e "$output_dir" ]] \
    || fail "$label missing VM baseline created output directory: $output_dir"
}

assert_invalid_sample_bounds() {
  local label="$1"
  local output_dir="$2"
  local expected="$3"
  shift 3
  local runner="bridgevm"
  if [[ "${1:-}" != --* ]]; then
    runner="$1"
    shift
  fi

  local stderr="$STORE/$label.stderr"
  if "$runner" performance sample "$VM_NAME" --output "$output_dir" "$@" >"$STORE/$label.stdout" 2>"$stderr"; then
    fail "$label unexpectedly accepted invalid performance sample bounds"
  fi
  grep -q "$expected" "$stderr" \
    || fail "$label stderr did not include '$expected': $(cat "$stderr")"
  [[ ! -e "$output_dir" ]] \
    || fail "$label invalid bounds created output directory: $output_dir"
}

assert_missing_vm_sample_rejected() {
  local label="$1"
  local output_dir="$2"
  shift 2

  local stderr="$STORE/$label.stderr"
  if "$@" performance sample missing-vm --output "$output_dir" --artifact-bytes 1024 --iterations 1 >"$STORE/$label.stdout" 2>"$stderr"; then
    fail "$label unexpectedly accepted missing VM performance sample"
  fi
  grep -q "VM not found" "$stderr" \
    || fail "$label stderr did not report missing VM: $(cat "$stderr")"
  [[ ! -e "$output_dir" ]] \
    || fail "$label missing VM sample created output directory: $output_dir"
}

trap stop_daemon EXIT

assert_missing_vm_sample_rejected \
  "local-missing-vm" \
  "$STORE/local-missing-vm-output" \
  bridgevm

assert_missing_vm_baseline_rejected \
  "local-missing-vm-baseline" \
  "$STORE/local-missing-vm-baseline-output" \
  bridgevm

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

LOCAL_BASELINE_OUTPUT_DIR="$STORE/local-performance-baseline"
local_baseline_output="$(
  bridgevm performance baseline "$VM_NAME" \
    --output "$LOCAL_BASELINE_OUTPUT_DIR"
)"
local_baseline_artifact="$(extract_line_value "Artifact" "$local_baseline_output")"
assert_baseline_contract "local CLI" "$local_baseline_output" "$local_baseline_artifact"

LOCAL_OUTPUT_DIR="$STORE/local-performance"
local_output="$(
  bridgevm performance sample "$VM_NAME" \
    --output "$LOCAL_OUTPUT_DIR" \
    --artifact-bytes 4096 \
    --iterations 1
)"
local_artifact="$(extract_line_value "Artifact" "$local_output")"
local_probe="$(extract_line_value "Probe" "$local_output")"
assert_sample_contract "local CLI" "$local_output" "$local_artifact" "$local_probe"

SYNC_OUTPUT_DIR="$STORE/sync-performance"
sync_output="$(
  bridgevm performance sample "$VM_NAME" \
    --output "$SYNC_OUTPUT_DIR" \
    --artifact-bytes 1024 \
    --iterations 3 \
    --sync
)"
sync_artifact="$(extract_line_value "Artifact" "$sync_output")"
sync_probe="$(extract_line_value "Probe" "$sync_output")"
assert_sample_contract "sync CLI" "$sync_output" "$sync_artifact" "$sync_probe" "$VM_NAME" "$BUNDLE" 1024 3 true
grep -q '"iteration": 3' "$sync_artifact" \
  || fail "sync CLI sample metadata did not record third iteration"
grep -q '"sync": true' "$sync_artifact" \
  || fail "sync CLI sample metadata did not record synced iterations"

assert_invalid_sample_bounds \
  "invalid-zero-iterations" \
  "$STORE/invalid-zero-iterations-output" \
  "performance sample iterations must be greater than zero" \
  --artifact-bytes 1024 \
  --iterations 0

assert_invalid_sample_bounds \
  "invalid-too-many-iterations" \
  "$STORE/invalid-too-many-iterations-output" \
  "performance sample iterations is too large" \
  --artifact-bytes 1024 \
  --iterations 101

assert_invalid_sample_bounds \
  "invalid-too-large-artifact" \
  "$STORE/invalid-too-large-artifact-output" \
  "performance sample artifact is too large" \
  --artifact-bytes 67108865 \
  --iterations 1

assert_invalid_sample_bounds \
  "invalid-total-artifact-bytes" \
  "$STORE/invalid-total-artifact-bytes-output" \
  "performance sample total artifact bytes is too large" \
  --artifact-bytes 67108864 \
  --iterations 5

FAKE_BIN="$STORE/fake-bin"
mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/qemu-img" <<'FAKE_QEMU_IMG'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "info" && "$2" == "--output=json" ]]; then
  printf '{"format":"qcow2","virtual-size":1048576,"actual-size":16}\n'
  exit 0
fi
echo "unexpected qemu-img invocation: $*" >&2
exit 64
FAKE_QEMU_IMG
chmod +x "$FAKE_BIN/qemu-img"

bridgevm create "$QEMU_VM_NAME" --os ubuntu --arch x86_64 --mode compatibility --disk 1MiB >/dev/null
mkdir -p "$QEMU_BUNDLE/disks"
printf 'fake-qcow2-probe\n' >"$QEMU_BUNDLE/disks/root.qcow2"

QEMU_OUTPUT_DIR="$STORE/qemu-img-performance"
qemu_output="$(
  PATH="$FAKE_BIN:$PATH" bridgevm performance sample "$QEMU_VM_NAME" \
    --output "$QEMU_OUTPUT_DIR" \
    --artifact-bytes 512 \
    --iterations 2
)"
qemu_artifact="$(extract_line_value "Artifact" "$qemu_output")"
qemu_probe="$(extract_line_value "Probe" "$qemu_output")"
assert_sample_contract "fake qemu-img CLI" "$qemu_output" "$qemu_artifact" "$qemu_probe" "$QEMU_VM_NAME" "$QEMU_BUNDLE" 512 2 false
grep -q 'Measurement: disk_inspect_duration_microseconds=' <<<"$qemu_output" \
  || fail "fake qemu-img sample output omitted disk inspect duration measurement"
grep -q '"name": "disk_inspect_duration_microseconds"' "$qemu_artifact" \
  || fail "fake qemu-img sample metadata omitted disk inspect duration measurement"
grep -q '"source": "host.qemu-img.info"' "$qemu_artifact" \
  || fail "fake qemu-img sample metadata omitted host qemu-img source"
grep -q 'disk inspect duration measures host qemu-img info execution, not guest disk I/O' "$qemu_artifact" \
  || fail "fake qemu-img sample metadata omitted disk inspect note"
grep -q '"format": "qcow2"' "$QEMU_BUNDLE/metadata/last-disk-inspect.json" \
  || fail "fake qemu-img did not write disk inspect metadata"

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

assert_missing_vm_sample_rejected \
  "socket-missing-vm" \
  "$STORE/socket-missing-vm-output" \
  bridgevm_socket

assert_missing_vm_baseline_rejected \
  "socket-missing-vm-baseline" \
  "$STORE/socket-missing-vm-baseline-output" \
  bridgevm_socket

assert_invalid_sample_bounds \
  "socket-invalid-zero-iterations" \
  "$STORE/socket-invalid-zero-iterations-output" \
  "performance sample iterations must be greater than zero" \
  bridgevm_socket \
  --artifact-bytes 1024 \
  --iterations 0

SOCKET_BASELINE_OUTPUT_DIR="$STORE/socket-performance-baseline"
socket_baseline_output="$(
  bridgevm_socket performance baseline "$VM_NAME" \
    --output "$SOCKET_BASELINE_OUTPUT_DIR"
)"
socket_baseline_artifact="$(extract_line_value "Artifact" "$socket_baseline_output")"
assert_baseline_contract "socket API" "$socket_baseline_output" "$socket_baseline_artifact"

SOCKET_OUTPUT_DIR="$STORE/socket-performance"
socket_output="$(
  bridgevm_socket performance sample "$VM_NAME" \
    --output "$SOCKET_OUTPUT_DIR" \
    --artifact-bytes 4096 \
    --iterations 1
)"
socket_artifact="$(extract_line_value "Artifact" "$socket_output")"
socket_probe="$(extract_line_value "Probe" "$socket_output")"
assert_sample_contract "socket API" "$socket_output" "$socket_artifact" "$socket_probe"

echo "PASS: performance baseline/sample CLI/socket integration smoke ($STORE)"
