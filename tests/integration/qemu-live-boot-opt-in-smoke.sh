#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
  exit 0
}

[[ "${BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1 to run the QEMU live boot smoke"
[[ -n "${BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED:-}" ]] || \
  skip "set BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED to prove guest boot progress from the serial log"

if ! command -v qemu-system-x86_64 >/dev/null 2>&1 && \
   ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  skip "qemu-system-x86_64 or qemu-system-aarch64 must be available on PATH"
fi

CREATED_STORE=0
if [[ -n "${BRIDGEVM_LIVE_QEMU_STORE:-}" ]]; then
  STORE="$BRIDGEVM_LIVE_QEMU_STORE"
else
  STORE="$(mktemp -d "/tmp/bridgevm-live-qemu.XXXXXX")"
  CREATED_STORE=1
fi

VM_NAME="${BRIDGEVM_LIVE_QEMU_VM:-live-qemu}"
VM_ARCH="${BRIDGEVM_LIVE_QEMU_ARCH:-x86_64}"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
EVIDENCE_DIR="${BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR:-$STORE/evidence}"
SERIAL_EXPECTED="$BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED"
TIMEOUT_SECONDS="${BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS:-60}"
QMP_TRANSCRIPT="$EVIDENCE_DIR/qmp-transcript.jsonl"
EVIDENCE_QEMU_LOG="$EVIDENCE_DIR/qemu.log"
EVIDENCE_SERIAL_LOG="$EVIDENCE_DIR/serial.log"
EVIDENCE_JSON="$EVIDENCE_DIR/qemu-live-evidence.json"
RUN_OUTPUT="$EVIDENCE_DIR/bridgevm-run.output"
READINESS_OUTPUT="$EVIDENCE_DIR/bridgevm-readiness-record.output"
SUMMARY_FILE="$EVIDENCE_DIR/SUMMARY.txt"
FIXTURE_MANIFEST="$EVIDENCE_DIR/fixture-manifest.json"
ENVIRONMENT_FILE="$EVIDENCE_DIR/environment.txt"

[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]] || \
  skip "BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS must be a positive integer"
case "$VM_ARCH" in
  x86_64|amd64|arm64|aarch64) ;;
  *) skip "BRIDGEVM_LIVE_QEMU_ARCH must be x86_64, amd64, arm64, or aarch64" ;;
esac

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

json_string() {
  printf '%s' "$1" | perl -pe 's/\\/\\\\/g; s/"/\\"/g; s/\n/\\n/g'
}

file_size() {
  wc -c <"$1" | tr -d ' '
}

file_sha256() {
  shasum -a 256 "$1" | awk '{print $1}'
}

file_json_entry() {
  local label="$1"
  local path="$2"

  if [[ -n "$path" && -f "$path" ]]; then
    cat <<EOF
    "$label": {
      "path": "$(json_string "$path")",
      "exists": true,
      "bytes": $(file_size "$path"),
      "sha256": "$(file_sha256 "$path")"
    }
EOF
  else
    cat <<EOF
    "$label": {
      "path": "$(json_string "$path")",
      "exists": false,
      "bytes": null,
      "sha256": null
    }
EOF
  fi
}

write_environment_evidence() {
  mkdir -p "$EVIDENCE_DIR"
  {
    echo "BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=${BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START:-}"
    echo "BRIDGEVM_LIVE_QEMU_STORE=${BRIDGEVM_LIVE_QEMU_STORE:-}"
    echo "BRIDGEVM_LIVE_QEMU_VM=${BRIDGEVM_LIVE_QEMU_VM:-}"
    echo "BRIDGEVM_LIVE_QEMU_ARCH=$VM_ARCH"
    echo "BRIDGEVM_LIVE_QEMU_QCOW2_DISK=${BRIDGEVM_LIVE_QEMU_QCOW2_DISK:-}"
    echo "BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR=${BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR:-}"
    echo "BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=$SERIAL_EXPECTED"
    echo "BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS=$TIMEOUT_SECONDS"
  } >"$ENVIRONMENT_FILE"
}

write_fixture_manifest() {
  local active_disk=""
  if [[ -f "$BUNDLE/metadata/runner.json" ]]; then
    active_disk="$(python3 - "$BUNDLE/metadata/runner.json" <<'PY'
import json
import sys
from pathlib import Path

runner = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
disk = runner.get("active_disk") or runner.get("disk") or {}
print(disk.get("path") or "")
PY
)"
  fi

  {
    printf '{\n'
    printf '  "generated_at_utc": "%s",\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf '  "store": "%s",\n' "$(json_string "$STORE")"
    printf '  "bundle": "%s",\n' "$(json_string "$BUNDLE")"
    printf '  "created_store": %s,\n' "$([[ "$CREATED_STORE" == "1" ]] && echo true || echo false)"
    file_json_entry "source_qcow2_disk" "${BRIDGEVM_LIVE_QEMU_QCOW2_DISK:-}"
    printf ',\n'
    file_json_entry "bundle_active_disk" "$active_disk"
    printf '\n}\n'
  } >"$FIXTURE_MANIFEST"
}

write_summary() {
  local status="$1"
  mkdir -p "$EVIDENCE_DIR"

  local serial_state="not checked"
  if [[ -f "$EVIDENCE_SERIAL_LOG" ]] && grep -Fq "$SERIAL_EXPECTED" "$EVIDENCE_SERIAL_LOG"; then
    serial_state="required sentinel found: $SERIAL_EXPECTED"
  elif [[ -f "$BUNDLE/logs/serial.log" ]] && grep -Fq "$SERIAL_EXPECTED" "$BUNDLE/logs/serial.log"; then
    serial_state="required sentinel found in bundle log: $SERIAL_EXPECTED"
  elif [[ -n "$SERIAL_EXPECTED" ]]; then
    serial_state="required sentinel not found yet: $SERIAL_EXPECTED"
  fi

  {
    echo "QEMU live boot opt-in smoke: $status"
    echo "Generated at UTC: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "Store: $STORE"
    echo "Bundle: $BUNDLE"
    echo "Evidence JSON: $EVIDENCE_JSON"
    echo "QEMU log: $EVIDENCE_QEMU_LOG"
    echo "Serial log: $EVIDENCE_SERIAL_LOG"
    echo "QMP transcript: $QMP_TRANSCRIPT"
    echo "Serial evidence: $serial_state"
    echo "Timeout seconds: $TIMEOUT_SECONDS"
    echo "Fixture manifest: $FIXTURE_MANIFEST"
    echo "Environment: $ENVIRONMENT_FILE"
    echo "Run output: $RUN_OUTPUT"
    echo "Readiness record output: $READINESS_OUTPUT"
  } >"$SUMMARY_FILE"
}

fail() {
  write_fixture_manifest || true
  write_summary "failed" || true
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  echo "Evidence directory: $EVIDENCE_DIR" >&2
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

cleanup() {
  if [[ -f "$BUNDLE/metadata/runner.json" ]]; then
    bridgevm stop "$VM_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

mkdir -p "$EVIDENCE_DIR"
write_environment_evidence
write_fixture_manifest
write_summary "prepared"

if [[ ! -d "$BUNDLE" ]]; then
  if [[ -n "${BRIDGEVM_LIVE_QEMU_QCOW2_DISK:-}" ]]; then
    [[ -f "$BRIDGEVM_LIVE_QEMU_QCOW2_DISK" ]] || \
      skip "BRIDGEVM_LIVE_QEMU_QCOW2_DISK does not exist: $BRIDGEVM_LIVE_QEMU_QCOW2_DISK"
    bridgevm create "$VM_NAME" --os ubuntu --arch "$VM_ARCH" --mode compatibility >/dev/null
    mkdir -p "$BUNDLE/disks"
    cp "$BRIDGEVM_LIVE_QEMU_QCOW2_DISK" "$BUNDLE/disks/root.qcow2"
  else
    skip "set BRIDGEVM_LIVE_QEMU_STORE/BRIDGEVM_LIVE_QEMU_VM for an existing Compatibility Mode VM, or BRIDGEVM_LIVE_QEMU_QCOW2_DISK for a disposable VM"
  fi
fi

run_output="$(bridgevm run "$VM_NAME" --spawn)" || fail "bridgevm run --spawn failed"
printf '%s\n' "$run_output" >"$RUN_OUTPUT"
assert_contains "$run_output" "Dry run: false" "QEMU live run"
assert_contains "$run_output" "Command:" "QEMU live run"

if ! python3 - "$BUNDLE" "$TIMEOUT_SECONDS" "$SERIAL_EXPECTED" "$QMP_TRANSCRIPT" <<'PY'
import json
import socket
import sys
import time
from pathlib import Path

bundle = Path(sys.argv[1])
timeout = int(sys.argv[2])
serial_expected = sys.argv[3]
transcript = Path(sys.argv[4])
runner = json.loads((bundle / "metadata" / "runner.json").read_text(encoding="utf-8"))
command = runner["command"]

def option_value(args, flag):
    for index, arg in enumerate(args):
        if arg == flag and index + 1 < len(args):
            return args[index + 1]
    raise SystemExit(f"missing {flag} in runner command: {args!r}")

qmp_arg = option_value(command, "-qmp")
if not qmp_arg.startswith("unix:"):
    raise SystemExit(f"QMP argument is not unix: {qmp_arg}")
qmp_socket = Path(qmp_arg.removeprefix("unix:").split(",", 1)[0])
serial_log = bundle / "logs" / "serial.log"
deadline = time.monotonic() + timeout
last_error = None
transcript.parent.mkdir(parents=True, exist_ok=True)

while time.monotonic() < deadline:
    if serial_log.exists() and serial_expected in serial_log.read_text(errors="replace"):
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(1.0)
            sock.connect(str(qmp_socket))
            stream = sock.makefile("rwb")
            transcript_lines = []
            greeting = stream.readline().decode("utf-8")
            transcript_lines.append(greeting)
            capabilities_command = '{"execute":"qmp_capabilities","id":"bridgevm-live-capabilities"}\n'
            transcript_lines.append(capabilities_command)
            stream.write(capabilities_command.encode("utf-8"))
            stream.flush()

            while True:
                line = stream.readline().decode("utf-8")
                transcript_lines.append(line)
                parsed = json.loads(line)
                if parsed.get("id") == "bridgevm-live-capabilities" and "return" in parsed:
                    break

            status_command = '{"execute":"query-status","id":"bridgevm-live-query-status"}\n'
            transcript_lines.append(status_command)
            stream.write(status_command.encode("utf-8"))
            stream.flush()
            status = ""
            while True:
                line = stream.readline().decode("utf-8")
                transcript_lines.append(line)
                parsed = json.loads(line)
                if parsed.get("id") == "bridgevm-live-query-status" and "return" in parsed:
                    status = line
                    break
            sock.close()
            with transcript.open("w", encoding="utf-8") as handle:
                handle.writelines(transcript_lines)
            parsed = json.loads(status)
            result = parsed.get("return", {})
            if result.get("running") is True and result.get("status") == "running":
                raise SystemExit(0)
            last_error = f"QMP query-status was not running: {status.strip()}"
        except SystemExit:
            raise
        except Exception as exc:
            last_error = str(exc)
    time.sleep(0.25)

raise SystemExit(
    f"timed out waiting for serial sentinel {serial_expected!r} and running QMP status"
    + (f": {last_error}" if last_error else "")
)
PY
then
  fail "timed out waiting for serial sentinel and running QMP status; see $QMP_TRANSCRIPT"
fi

if ! python3 - "$BUNDLE" "$EVIDENCE_DIR" "$SERIAL_EXPECTED" <<'PY'
import json
import shutil
import sys
from pathlib import Path

bundle = Path(sys.argv[1])
evidence = Path(sys.argv[2])
serial_expected = sys.argv[3]
runner = json.loads((bundle / "metadata" / "runner.json").read_text(encoding="utf-8"))
command = runner["command"]
qemu_log = Path(runner["log_path"])
serial_log = bundle / "logs" / "serial.log"
qmp_transcript = evidence / "qmp-transcript.jsonl"

def option_value(args, flag):
    for index, arg in enumerate(args):
        if arg == flag and index + 1 < len(args):
            return args[index + 1]
    raise SystemExit(f"missing {flag} in runner command: {args!r}")

qmp_arg = option_value(command, "-qmp")
qmp_socket = qmp_arg.removeprefix("unix:").split(",", 1)[0]
command_line = " ".join(command)

evidence.mkdir(parents=True, exist_ok=True)
evidence_qemu = evidence / "qemu.log"
evidence_serial = evidence / "serial.log"
if qemu_log.exists():
    shutil.copyfile(qemu_log, evidence_qemu)
else:
    evidence_qemu.write_text("", encoding="utf-8")
with evidence_qemu.open("a", encoding="utf-8") as handle:
    handle.write(f"\nBridgeVM live QEMU evidence\n")
    handle.write(f"Command: {command_line}\n")
    handle.write(f"QMP socket: {qmp_socket}\n")
    handle.write("QMP status: running\n")
shutil.copyfile(serial_log, evidence_serial)

def sha(path):
    import hashlib
    return hashlib.sha256(path.read_bytes()).hexdigest()

metadata = {
    "proven": True,
    "backend": "qemu",
    "vm_name": runner.get("disk", {}).get("vm", "") or bundle.name.removesuffix(".vmbridge"),
    "boot_mode": "compatibility",
    "disk_format": runner.get("active_disk", runner.get("disk", {})).get("format", "qcow2"),
    "network": "nat",
    "command": command,
    "qmp": {
        "running": True,
        "status": "running",
        "socket": qmp_socket,
    },
    "serial_sentinel": serial_expected,
    "artifacts": {
        "qemu_log": {
            "path": "qemu.log",
            "sha256": sha(evidence_qemu),
        },
        "serial_log": {
            "path": "serial.log",
            "sha256": sha(evidence_serial),
        },
        "qmp_transcript": {
            "path": "qmp-transcript.jsonl",
            "sha256": sha(qmp_transcript),
        },
    },
}
(evidence / "qemu-live-evidence.json").write_text(
    json.dumps(metadata, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
then
  fail "QEMU live evidence artifact generation failed"
fi

write_fixture_manifest
write_summary "evidence-captured"

readiness_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$EVIDENCE_DIR" --record-live-evidence)" || \
  fail "BridgeVM readiness live evidence recording failed"
printf '%s\n' "$readiness_output" >"$READINESS_OUTPUT"
assert_contains "$readiness_output" "Live evidence: verified (" "recorded QEMU evidence"
assert_contains "$readiness_output" "recorded preserved live evidence bundle:" "recorded QEMU evidence"
assert_contains "$readiness_output" "Live evidence QMP: proven=true" "recorded QEMU evidence"
assert_contains "$readiness_output" "Live evidence serial sentinel: required=true proven=true" "recorded QEMU evidence"
write_summary "passed"

echo "PASS: QEMU live boot opt-in smoke ($STORE)"
echo "Evidence directory: $EVIDENCE_DIR"
echo "Summary: $SUMMARY_FILE"

if [[ "$CREATED_STORE" == "1" ]]; then
  echo "Disposable store preserved for review: $STORE"
fi
