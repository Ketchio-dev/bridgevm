#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-qemu-live-evidence.XXXXXX")"
VM_NAME="qemu-live-evidence"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
    *) ;;
  esac
}

cleanup() {
  rm -rf "$STORE"
}

trap cleanup EXIT

make_qemu_live_evidence_bundle() {
  local evidence="$1"
  local bundle="$2"
  local vm="$3"
  local request_id="qemu-guest-tools-1"

  mkdir -p "$evidence" "$bundle/logs"
  printf 'qemu-system-x86_64 started %s\nCommand: qemu-system-x86_64 -name %s -qmp unix:%s/metadata/qmp.sock,server=on,wait=off\nQMP socket: %s/metadata/qmp.sock\nQMP status: running\n' "$vm" "$vm" "$bundle" "$bundle" >"$evidence/qemu.log"
  printf 'serial boot line\nbridgevm-qemu-ready\n' >"$evidence/serial.log"
  printf 'bridgevm-qemu-file-proof\n' >"$evidence/guest-tools-effect.txt"
  cat >"$evidence/qmp-transcript.jsonl" <<'EOF'
{"QMP":{"version":{"qemu":{"major":9,"minor":0,"micro":0},"package":"bridgevm-smoke"},"capabilities":[]}}
{"execute":"qmp_capabilities"}
{"return":{}}
{"execute":"query-status"}
{"return":{"running":true,"status":"running"}}
EOF

  local qemu_sha serial_sha qmp_transcript_sha guest_tools_effect_sha
  qemu_sha="$(shasum -a 256 "$evidence/qemu.log" | awk '{print $1}')"
  serial_sha="$(shasum -a 256 "$evidence/serial.log" | awk '{print $1}')"
  qmp_transcript_sha="$(shasum -a 256 "$evidence/qmp-transcript.jsonl" | awk '{print $1}')"
  guest_tools_effect_sha="$(shasum -a 256 "$evidence/guest-tools-effect.txt" | awk '{print $1}')"

  cat >"$evidence/guest-tools-effects.json" <<EOF
{
  "proven": true,
  "backend": "bridgevm-tools-linux",
  "command": {
    "request_id": "$request_id",
    "status": "ok"
  },
  "effects": [
    {
      "kind": "filesystem",
      "request_id": "$request_id",
      "ok": true,
      "expected_value": "bridgevm-qemu-file-proof",
      "observed_value": "bridgevm-qemu-file-proof",
      "observation": "guest wrote the requested probe file and reported success"
    },
    {
      "kind": "filesystem-artifact",
      "request_id": "$request_id",
      "ok": true,
      "artifact": "guest-tools-effect.txt",
      "sha256": "$guest_tools_effect_sha",
      "observation": "guest wrote a preserved artifact with matching SHA-256"
    }
  ]
}
EOF

  cat >"$evidence/qemu-live-evidence.json" <<EOF
{
  "proven": true,
  "backend": "qemu",
  "vm_name": "$vm",
  "boot_mode": "compatibility",
  "disk_format": "qcow2",
  "network": "nat",
  "command": ["qemu-system-x86_64", "-name", "$vm", "-qmp", "unix:$bundle/metadata/qmp.sock,server=on,wait=off"],
  "qmp": {
    "running": true,
    "status": "running",
    "socket": "$bundle/metadata/qmp.sock"
  },
  "serial_sentinel": "bridgevm-qemu-ready",
  "artifacts": {
    "qemu_log": {
      "path": "qemu.log",
      "sha256": "$qemu_sha"
    },
    "serial_log": {
      "path": "serial.log",
      "sha256": "$serial_sha"
    },
    "qmp_transcript": {
      "path": "qmp-transcript.jsonl",
      "sha256": "$qmp_transcript_sha"
    }
  }
}
EOF
}

assert_qemu_evidence_rejected() {
  local evidence="$1"
  local expected="$2"
  local label="$3"
  local output
  output="$(bridgevm readiness "$VM_NAME" --live-evidence "$evidence")"
  assert_contains "$output" "live-evidence-invalid:" "$label"
  assert_contains "$output" "$expected" "$label"
  assert_not_contains "$output" "Live evidence: verified" "$label"
}

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

GOOD_EVIDENCE="$STORE/evidence/good"
make_qemu_live_evidence_bundle "$GOOD_EVIDENCE" "$BUNDLE" "$VM_NAME"
good_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$GOOD_EVIDENCE")"
assert_contains "$good_output" "Live evidence: verified ($GOOD_EVIDENCE)" "good QEMU evidence"
assert_contains "$good_output" "Live evidence QMP: proven=true" "good QEMU evidence"
assert_contains "$good_output" "guest-tools-effects: required=true proven=true" "good QEMU evidence"

ID_TAGGED_QMP="$STORE/evidence/id-tagged-qmp"
cp -R "$GOOD_EVIDENCE" "$ID_TAGGED_QMP"
cat >"$ID_TAGGED_QMP/qmp-transcript.jsonl" <<'EOF'
{"QMP":{"version":{"qemu":{"major":9,"minor":0,"micro":0},"package":"bridgevm-smoke"},"capabilities":[]}}
{"execute":"qmp_capabilities","id":"bridgevm-live-capabilities"}
{"return":{},"id":"bridgevm-live-capabilities"}
{"event":"RESUME","data":{"status":"running"}}
{"execute":"query-status","id":"bridgevm-live-query-status"}
{"return":{"running":true,"status":"running"},"id":"bridgevm-live-query-status"}
EOF
id_tagged_qmp_sha="$(shasum -a 256 "$ID_TAGGED_QMP/qmp-transcript.jsonl" | awk '{print $1}')"
perl -0pi -e "s/(\"qmp_transcript\": \\{\\n      \"path\": \"qmp-transcript.jsonl\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$id_tagged_qmp_sha/" "$ID_TAGGED_QMP/qemu-live-evidence.json"
id_tagged_qmp_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$ID_TAGGED_QMP")"
assert_contains "$id_tagged_qmp_output" "Live evidence: verified ($ID_TAGGED_QMP)" "ID-tagged QMP evidence"
assert_contains "$id_tagged_qmp_output" "Live evidence QMP: proven=true" "ID-tagged QMP evidence"

NOT_PROVEN="$STORE/evidence/not-proven"
cp -R "$GOOD_EVIDENCE" "$NOT_PROVEN"
perl -0pi -e 's/"proven": true/"proven": false/' "$NOT_PROVEN/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$NOT_PROVEN" \
  "qemu-live-evidence.json does not mark live evidence as proven" \
  "not proven QEMU evidence"

WRONG_BACKEND="$STORE/evidence/wrong-backend"
cp -R "$GOOD_EVIDENCE" "$WRONG_BACKEND"
perl -0pi -e 's/"backend": "qemu"/"backend": "apple-virtualization-framework"/' "$WRONG_BACKEND/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$WRONG_BACKEND" \
  "backend is not qemu" \
  "wrong backend QEMU evidence"

WRONG_BOOT_MODE="$STORE/evidence/wrong-boot-mode"
cp -R "$GOOD_EVIDENCE" "$WRONG_BOOT_MODE"
perl -0pi -e 's/"boot_mode": "compatibility"/"boot_mode": "linux-kernel"/' "$WRONG_BOOT_MODE/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$WRONG_BOOT_MODE" \
  "boot_mode is not compatibility: linux-kernel" \
  "wrong boot mode QEMU evidence"

MISSING_NAME_COMMAND="$STORE/evidence/missing-name-command"
cp -R "$GOOD_EVIDENCE" "$MISSING_NAME_COMMAND"
perl -0pi -e 's/"-name"/"-no-name"/' "$MISSING_NAME_COMMAND/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$MISSING_NAME_COMMAND" \
  "qemu-live-evidence.json command is missing -name" \
  "missing command name QEMU evidence"

QMP_NOT_RUNNING="$STORE/evidence/qmp-not-running"
cp -R "$GOOD_EVIDENCE" "$QMP_NOT_RUNNING"
perl -0pi -e 's/"running": true/"running": false/' "$QMP_NOT_RUNNING/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$QMP_NOT_RUNNING" \
  "qmp.running is not true" \
  "QMP not running evidence"

QMP_SHUTDOWN="$STORE/evidence/qmp-shutdown"
cp -R "$GOOD_EVIDENCE" "$QMP_SHUTDOWN"
perl -0pi -e 's/"status": "running"/"status": "shutdown"/' "$QMP_SHUTDOWN/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$QMP_SHUTDOWN" \
  "qmp.status is not running: shutdown" \
  "QMP shutdown evidence"

MISSING_QMP_TRANSCRIPT="$STORE/evidence/missing-qmp-transcript"
cp -R "$GOOD_EVIDENCE" "$MISSING_QMP_TRANSCRIPT"
rm "$MISSING_QMP_TRANSCRIPT/qmp-transcript.jsonl"
assert_qemu_evidence_rejected \
  "$MISSING_QMP_TRANSCRIPT" \
  "artifacts.qmp_transcript artifact is not a file" \
  "missing QMP transcript evidence"

QMP_TRANSCRIPT_SHUTDOWN="$STORE/evidence/qmp-transcript-shutdown"
cp -R "$GOOD_EVIDENCE" "$QMP_TRANSCRIPT_SHUTDOWN"
perl -0pi -e 's/"running":true,"status":"running"/"running":false,"status":"shutdown"/' "$QMP_TRANSCRIPT_SHUTDOWN/qmp-transcript.jsonl"
qmp_shutdown_sha="$(shasum -a 256 "$QMP_TRANSCRIPT_SHUTDOWN/qmp-transcript.jsonl" | awk '{print $1}')"
perl -0pi -e "s/(\"qmp_transcript\": \\{\\n      \"path\": \"qmp-transcript.jsonl\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$qmp_shutdown_sha/" "$QMP_TRANSCRIPT_SHUTDOWN/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$QMP_TRANSCRIPT_SHUTDOWN" \
  "QMP transcript evidence missing running query-status response" \
  "QMP transcript shutdown evidence"

QMP_TRANSCRIPT_OUT_OF_ORDER="$STORE/evidence/qmp-transcript-out-of-order"
cp -R "$GOOD_EVIDENCE" "$QMP_TRANSCRIPT_OUT_OF_ORDER"
cat >"$QMP_TRANSCRIPT_OUT_OF_ORDER/qmp-transcript.jsonl" <<'EOF'
{"QMP":{"version":{"qemu":{"major":9,"minor":0,"micro":0},"package":"bridgevm-smoke"},"capabilities":[]}}
{"return":{"running":true,"status":"running"}}
{"execute":"query-status"}
EOF
qmp_out_of_order_sha="$(shasum -a 256 "$QMP_TRANSCRIPT_OUT_OF_ORDER/qmp-transcript.jsonl" | awk '{print $1}')"
perl -0pi -e "s/(\"qmp_transcript\": \\{\\n      \"path\": \"qmp-transcript.jsonl\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$qmp_out_of_order_sha/" "$QMP_TRANSCRIPT_OUT_OF_ORDER/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$QMP_TRANSCRIPT_OUT_OF_ORDER" \
  "QMP transcript evidence missing running query-status response" \
  "QMP transcript out-of-order evidence"

BAD_EXECUTABLE="$STORE/evidence/bad-executable"
cp -R "$GOOD_EVIDENCE" "$BAD_EXECUTABLE"
perl -0pi -e 's/"qemu-system-x86_64"/"not-qemu-system-x86_64"/' "$BAD_EXECUTABLE/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$BAD_EXECUTABLE" \
  "command[0] is not a supported qemu-system executable" \
  "bad executable evidence"

WRAPPER_EXECUTABLE="$STORE/evidence/wrapper-executable"
cp -R "$GOOD_EVIDENCE" "$WRAPPER_EXECUTABLE"
perl -0pi -e 's/"qemu-system-x86_64"/"qemu-system-x86_64-wrapper"/' "$WRAPPER_EXECUTABLE/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$WRAPPER_EXECUTABLE" \
  "command[0] is not a supported qemu-system executable" \
  "wrapper executable evidence"

SELF_CONSISTENT_WRONG_QMP="$STORE/evidence/self-consistent-wrong-qmp"
cp -R "$GOOD_EVIDENCE" "$SELF_CONSISTENT_WRONG_QMP"
perl -0pi -e 's#unix:[^"]+/metadata/qmp.sock,server=on,wait=off#unix:/tmp/bridgevm-wrong-qmp.sock,server=on,wait=off#' "$SELF_CONSISTENT_WRONG_QMP/qemu-live-evidence.json"
perl -0pi -e 's#"socket": "[^"]+"#"socket": "/tmp/bridgevm-wrong-qmp.sock"#' "$SELF_CONSISTENT_WRONG_QMP/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$SELF_CONSISTENT_WRONG_QMP" \
  "does not match expected VM QMP socket" \
  "self-consistent wrong QMP socket evidence"

COMMAND_QMP_MISMATCH="$STORE/evidence/command-qmp-mismatch"
cp -R "$GOOD_EVIDENCE" "$COMMAND_QMP_MISMATCH"
perl -0pi -e 's#unix:[^"]+/metadata/qmp.sock,server=on,wait=off#unix:/tmp/bridgevm-command-qmp.sock,server=on,wait=off#' "$COMMAND_QMP_MISMATCH/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$COMMAND_QMP_MISMATCH" \
  "does not match command -qmp" \
  "command QMP mismatch evidence"

COMMAND_QMP_LOOSE_SUFFIX="$STORE/evidence/command-qmp-loose-suffix"
cp -R "$GOOD_EVIDENCE" "$COMMAND_QMP_LOOSE_SUFFIX"
perl -0pi -e 's/server=on,wait=off/server=on,wait=off,extra=1/' "$COMMAND_QMP_LOOSE_SUFFIX/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$COMMAND_QMP_LOOSE_SUFFIX" \
  "does not match command -qmp" \
  "command QMP loose suffix evidence"

EMPTY_QMP_SOCKET="$STORE/evidence/empty-qmp-socket"
cp -R "$GOOD_EVIDENCE" "$EMPTY_QMP_SOCKET"
perl -0pi -e 's#"socket": "[^"]+"#"socket": ""#' "$EMPTY_QMP_SOCKET/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$EMPTY_QMP_SOCKET" \
  "qmp.socket is empty" \
  "empty QMP socket evidence"

THIN_QEMU_LOG="$STORE/evidence/thin-qemu-log"
cp -R "$GOOD_EVIDENCE" "$THIN_QEMU_LOG"
printf 'qemu-system-x86_64 started %s\nQMP status: running\n' "$VM_NAME" >"$THIN_QEMU_LOG/qemu.log"
thin_qemu_sha="$(shasum -a 256 "$THIN_QEMU_LOG/qemu.log" | awk '{print $1}')"
perl -0pi -e "s/(\"qemu_log\": \\{\\n      \"path\": \"qemu.log\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$thin_qemu_sha/" "$THIN_QEMU_LOG/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$THIN_QEMU_LOG" \
  "QEMU log evidence missing" \
  "thin QEMU log evidence"

LOOSE_QEMU_LOG="$STORE/evidence/loose-qemu-log"
cp -R "$GOOD_EVIDENCE" "$LOOSE_QEMU_LOG"
printf 'qemu-system-x86_64 started %s\nmetadata socket %s/metadata/qmp.sock\nQMP status: running\n' "$VM_NAME" "$BUNDLE" >"$LOOSE_QEMU_LOG/qemu.log"
loose_qemu_sha="$(shasum -a 256 "$LOOSE_QEMU_LOG/qemu.log" | awk '{print $1}')"
perl -0pi -e "s/(\"qemu_log\": \\{\\n      \"path\": \"qemu.log\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$loose_qemu_sha/" "$LOOSE_QEMU_LOG/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$LOOSE_QEMU_LOG" \
  "Command: qemu-system-x86_64 -name $VM_NAME -qmp unix:$BUNDLE/metadata/qmp.sock,server=on,wait=off" \
  "loose QEMU log evidence"

SHA_MISMATCH="$STORE/evidence/sha-mismatch"
cp -R "$GOOD_EVIDENCE" "$SHA_MISMATCH"
perl -0pi -e 's/"sha256": "[0-9a-f]{64}"/"sha256": "0000000000000000000000000000000000000000000000000000000000000000"/' "$SHA_MISMATCH/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$SHA_MISMATCH" \
  "artifacts.qemu_log.sha256 does not match artifact" \
  "SHA mismatch evidence"

OUTSIDE_SERIAL_ARTIFACT="$STORE/evidence/outside-serial-artifact"
cp -R "$GOOD_EVIDENCE" "$OUTSIDE_SERIAL_ARTIFACT"
perl -0pi -e 's#"path": "serial.log"#"path": "../serial.log"#' "$OUTSIDE_SERIAL_ARTIFACT/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$OUTSIDE_SERIAL_ARTIFACT" \
  "artifacts.serial_log path must be relative and stay inside the evidence bundle" \
  "outside serial artifact evidence"

SYMLINK_SERIAL_ARTIFACT="$STORE/evidence/symlink-serial-artifact"
cp -R "$GOOD_EVIDENCE" "$SYMLINK_SERIAL_ARTIFACT"
printf 'serial boot line\nbridgevm-qemu-ready\n' >"$STORE/outside-serial.log"
rm "$SYMLINK_SERIAL_ARTIFACT/serial.log"
ln -s "$STORE/outside-serial.log" "$SYMLINK_SERIAL_ARTIFACT/serial.log"
symlink_serial_sha="$(shasum -a 256 "$STORE/outside-serial.log" | awk '{print $1}')"
perl -0pi -e "s/(\"serial_log\": \\{\\n      \"path\": \"serial.log\",\\n      \"sha256\": \")[0-9a-f]{64}/\${1}$symlink_serial_sha/" "$SYMLINK_SERIAL_ARTIFACT/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$SYMLINK_SERIAL_ARTIFACT" \
  "artifacts.serial_log artifact must not be a symlink" \
  "symlink serial artifact evidence"

MISSING_SERIAL_SENTINEL="$STORE/evidence/missing-serial-sentinel"
cp -R "$GOOD_EVIDENCE" "$MISSING_SERIAL_SENTINEL"
perl -0pi -e 's/"serial_sentinel": "bridgevm-qemu-ready"/"serial_sentinel": "bridgevm-qemu-missing"/' "$MISSING_SERIAL_SENTINEL/qemu-live-evidence.json"
assert_qemu_evidence_rejected \
  "$MISSING_SERIAL_SENTINEL" \
  "QEMU serial log evidence missing" \
  "missing serial sentinel evidence"

GUEST_TOOLS_NOT_PROVEN="$STORE/evidence/guest-tools-not-proven"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_NOT_PROVEN"
perl -0pi -e 's/"proven": true/"proven": false/' "$GUEST_TOOLS_NOT_PROVEN/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_NOT_PROVEN" \
  "guest-tools-effects.json does not mark effects as proven" \
  "guest-tools not proven evidence"

GUEST_TOOLS_COMMAND_FAILED="$STORE/evidence/guest-tools-command-failed"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_COMMAND_FAILED"
perl -0pi -e 's/"status": "ok"/"status": "error"/' "$GUEST_TOOLS_COMMAND_FAILED/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_COMMAND_FAILED" \
  "guest-tools-effects.json command status is not ok: error" \
  "guest-tools command failed evidence"

GUEST_TOOLS_EFFECT_FAILED="$STORE/evidence/guest-tools-effect-failed"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_EFFECT_FAILED"
perl -0pi -e 's/"ok": true/"ok": false/' "$GUEST_TOOLS_EFFECT_FAILED/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_EFFECT_FAILED" \
  "guest-tools-effects.json effects[0] is not ok" \
  "guest-tools effect failed evidence"

GUEST_TOOLS_REQUEST_MISMATCH="$STORE/evidence/guest-tools-request-mismatch"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_REQUEST_MISMATCH"
perl -0pi -e 's/"request_id": "qemu-guest-tools-1",\n      "ok": true/"request_id": "qemu-guest-tools-other",\n      "ok": true/' "$GUEST_TOOLS_REQUEST_MISMATCH/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_REQUEST_MISMATCH" \
  "guest-tools-effects.json effects[0] request_id does not match command" \
  "guest-tools request mismatch evidence"

GUEST_TOOLS_EFFECT_UNOBSERVABLE="$STORE/evidence/guest-tools-effect-unobservable"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_EFFECT_UNOBSERVABLE"
perl -0pi -e 's/,\n      "expected_value": "bridgevm-qemu-file-proof",\n      "observed_value": "bridgevm-qemu-file-proof"//' "$GUEST_TOOLS_EFFECT_UNOBSERVABLE/guest-tools-effects.json"
perl -0pi -e 's/,\n    \{\n      "kind": "filesystem-artifact",\n      "request_id": "qemu-guest-tools-1",\n      "ok": true,\n      "artifact": "guest-tools-effect.txt",\n      "sha256": "[0-9a-f]{64}",\n      "observation": "guest wrote a preserved artifact with matching SHA-256"\n    \}//' "$GUEST_TOOLS_EFFECT_UNOBSERVABLE/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_EFFECT_UNOBSERVABLE" \
  "guest-tools-effects.json effects[0] needs expected_value/observed_value or artifact/sha256 evidence" \
  "guest-tools unobservable effect evidence"

GUEST_TOOLS_VALUE_ONLY="$STORE/evidence/guest-tools-value-only"
cp -R "$GOOD_EVIDENCE" "$GUEST_TOOLS_VALUE_ONLY"
perl -0pi -e 's/,\n    \{\n      "kind": "filesystem-artifact",\n      "request_id": "qemu-guest-tools-1",\n      "ok": true,\n      "artifact": "guest-tools-effect.txt",\n      "sha256": "[0-9a-f]{64}",\n      "observation": "guest wrote a preserved artifact with matching SHA-256"\n    \}//' "$GUEST_TOOLS_VALUE_ONLY/guest-tools-effects.json"
assert_qemu_evidence_rejected \
  "$GUEST_TOOLS_VALUE_ONLY" \
  "guest-tools-effects.json needs at least one artifact/sha256-backed effect" \
  "guest-tools value-only evidence"

echo "PASS: QEMU live evidence verifier smoke ($STORE)"
