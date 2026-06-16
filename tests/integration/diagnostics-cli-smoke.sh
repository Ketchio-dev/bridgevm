#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-diag.XXXXXX")"
VM_NAME="legacy-linux"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
LOCAL_OUTPUT="$STORE/local-diagnostics"
SOCKET_OUTPUT="$STORE/socket-diagnostics"

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

extract_output_path() {
  sed -nE 's/^Output: (.*)$/\1/p' | tail -n 1
}

assert_contains() {
  local needle="$1"
  local file="$2"
  grep -Fq "$needle" "$file" || fail "$file did not contain $needle"
}

assert_not_contains() {
  local needle="$1"
  local file="$2"
  if grep -Fq "$needle" "$file"; then
    fail "$file unexpectedly contained $needle"
  fi
}

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded: $output"
  fi
  case "$output" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $output" ;;
  esac
}

assert_no_bundle_created() {
  local label="$1"
  local output_root="$2"

  if find "$output_root" -mindepth 1 -maxdepth 1 -type d -name 'bridgevm-diagnostics-*' -print -quit 2>/dev/null | grep -q .; then
    fail "$label created a diagnostic bundle for a missing VM"
  fi
}

assert_metadata_paths_are_safe() {
  local manifest="$1"
  local label="$2"

  python3 - "$manifest" "$label" <<'PY'
import json
import pathlib
import sys

manifest = pathlib.Path(sys.argv[1])
label = sys.argv[2]
data = json.loads(manifest.read_text())
files = data.get("files")
if not isinstance(files, list) or not files:
    raise SystemExit(f"{label} diagnostic metadata did not include a non-empty files list")
for item in files:
    path = pathlib.PurePosixPath(item)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit(f"{label} diagnostic metadata included unsafe path: {item}")
    if item.endswith(".sock") or item.endswith(".lock"):
        raise SystemExit(f"{label} diagnostic metadata included excluded path: {item}")
PY
}

validate_bundle() {
  local output="$1"
  local stdout="$2"
  local label="$3"

  [[ -d "$output" ]] || fail "$label diagnostic bundle directory was not created: $output"
  [[ -f "$output/manifest.yaml" ]] || fail "$label bundle missing manifest.yaml"
  [[ -d "$output/logs" ]] || fail "$label bundle missing logs directory"
  [[ -d "$output/metadata" ]] || fail "$label bundle missing metadata directory"
  [[ -f "$output/diagnostic-bundle.json" ]] || fail "$label bundle missing diagnostic-bundle.json"
  [[ -f "$output/logs/qemu.log" ]] || fail "$label bundle missing logs/qemu.log"
  [[ -f "$output/metadata/guest-tools-token.json" ]] || fail "$label bundle missing guest-tools token metadata"
  [[ -f "$output/metadata/secrets.json" ]] || fail "$label bundle missing metadata/secrets.json"
  [[ -f "$output/metadata/boot-media/download-plan.json" ]] || fail "$label bundle missing boot-media download plan"
  [[ -f "$output/metadata/runner.json" ]] || fail "$label bundle missing runner readiness metadata"
  [[ -f "$output/metadata/apple-vz-launch.json" ]] || fail "$label bundle missing Apple VZ launch readiness metadata"

  assert_contains "Diagnostic bundle for $VM_NAME" "$stdout"
  assert_contains "Files: " "$stdout"
  assert_contains '"vm": "legacy-linux"' "$output/diagnostic-bundle.json"
  assert_contains "\"source\": \"$BUNDLE\"" "$output/diagnostic-bundle.json"
  assert_contains "\"output\": \"$output\"" "$output/diagnostic-bundle.json"
  assert_contains '"created_at_unix":' "$output/diagnostic-bundle.json"
  assert_contains '"manifest.yaml"' "$output/diagnostic-bundle.json"
  assert_contains '"metadata/guest-tools-token.json"' "$output/diagnostic-bundle.json"
  assert_contains '"metadata/qmp-supervisor.json"' "$output/diagnostic-bundle.json"
  assert_contains '"metadata/runner.json"' "$output/diagnostic-bundle.json"
  assert_contains '"metadata/apple-vz-launch.json"' "$output/diagnostic-bundle.json"
  assert_contains '"logs/qemu.log"' "$output/diagnostic-bundle.json"
  assert_contains '"diagnostic-bundle.json"' "$output/diagnostic-bundle.json"
  assert_metadata_paths_are_safe "$output/diagnostic-bundle.json" "$label"

  if grep -RFq "$TOKEN" "$output"; then
    fail "$label diagnostic bundle leaked the guest-tools token"
  fi

  assert_contains "<redacted>" "$output/metadata/guest-tools-token.json"
  assert_contains "<redacted>" "$output/logs/qemu.log"
  assert_contains "<redacted>" "$output/metadata/secrets.json"
  assert_not_contains "open-sesame" "$output/metadata/secrets.json"
  assert_not_contains "Bearer abc" "$output/metadata/secrets.json"
  assert_not_contains "sig=secret" "$output/metadata/boot-media/download-plan.json"
  assert_contains "https://example.invalid/ubuntu.iso?<redacted>#section" "$output/metadata/boot-media/download-plan.json"
  assert_contains "https://example.invalid/ubuntu.iso?<redacted>" "$output/metadata/boot-media/download-plan.json"
  assert_contains '"launch_spec_path": "metadata/apple-vz-launch.json"' "$output/metadata/runner.json"
  assert_contains '"ready": false' "$output/metadata/runner.json"
  assert_contains '"kind": "missing_boot_media"' "$output/metadata/runner.json"
  assert_contains '"remediation": "Import or verify boot media before launch."' "$output/metadata/runner.json"
  assert_contains '"readiness": {' "$output/metadata/apple-vz-launch.json"
  assert_contains '"ready": false' "$output/metadata/apple-vz-launch.json"
  assert_contains '"affected_path": "installer.iso"' "$output/metadata/apple-vz-launch.json"
  assert_contains '"live_evidence": "pending"' "$output/metadata/apple-vz-launch.json"

  [[ ! -e "$output/metadata/diagnostics.lock" ]] || fail "$label bundle included a lock file"
  [[ ! -e "$output/metadata/qmp.sock" ]] || fail "$label bundle included a socket file"
  [[ ! -e "$output/disks/root.qcow2" ]] || fail "$label bundle included a disk"
  [[ ! -e "$output/installer.iso" ]] || fail "$label bundle included installer media"

  if find "$output" \( -name "*.sock" -o -name "*.lock" \) -print -quit | grep -q .; then
    fail "$label bundle included socket or lock paths"
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

MISSING_LOCAL_OUTPUT="$STORE/missing-local-diagnostics"
assert_fails_contains \
  "local missing vm diagnostics" \
  "VM not found" \
  bridgevm diagnostics bundle missing-vm --output "$MISSING_LOCAL_OUTPUT"
assert_no_bundle_created "local missing vm diagnostics" "$MISSING_LOCAL_OUTPUT"

TOKEN="$(
  sed -nE 's/^[[:space:]]*"token"[[:space:]]*:[[:space:]]*"([^"]+)".*$/\1/p' \
    "$BUNDLE/metadata/guest-tools-token.json" | tail -n 1
)"
[[ -n "$TOKEN" ]] || fail "failed to read guest-tools token fixture"

printf "booted with token %s\n" "$TOKEN" >"$BUNDLE/logs/qemu.log"
printf '{"password":"open-sesame","nested":{"authorization":"Bearer abc"}}\n' \
  >"$BUNDLE/metadata/secrets.json"
mkdir -p "$BUNDLE/metadata/boot-media" "$BUNDLE/disks"
printf '{"url":"https://example.invalid/ubuntu.iso?sig=secret#section","command":["curl","https://example.invalid/ubuntu.iso?sig=secret"]}\n' \
  >"$BUNDLE/metadata/boot-media/download-plan.json"
printf '{"events":[{"event":"RESUME"}],"terminal_event":null,"envelopes_read":1,"limit_reached":false,"updated_at_unix":1}\n' \
  >"$BUNDLE/metadata/qmp-supervisor.json"
cat >"$BUNDLE/metadata/apple-vz-launch.json" <<'JSON'
{
  "vm": "legacy-linux",
  "backend": "apple-vz",
  "boot_mode": "linux-kernel",
  "boot_media": {
    "installer_image": {
      "path": "installer.iso",
      "exists": false
    }
  },
  "readiness": {
    "ready": false,
    "live_evidence": "pending",
    "blockers": [
      {
        "kind": "missing_boot_media",
        "affected_path": "installer.iso",
        "remediation": "Import or verify boot media before launch."
      }
    ],
    "notes": [
      "Boot media metadata is known, but local media is not present.",
      "live_evidence: pending"
    ]
  }
}
JSON
cat >"$BUNDLE/metadata/runner.json" <<'JSON'
{
  "backend": "lightvm",
  "state": "planned",
  "launch_spec_path": "metadata/apple-vz-launch.json",
  "launch_readiness": {
    "ready": false,
    "live_evidence": "pending",
    "blockers": [
      {
        "kind": "missing_boot_media",
        "affected_path": "installer.iso",
        "remediation": "Import or verify boot media before launch."
      }
    ],
    "notes": [
      "User-visible readiness is available without starting Apple VZ.",
      "live_evidence: pending"
    ]
  }
}
JSON
printf "locked\n" >"$BUNDLE/metadata/diagnostics.lock"
printf "socket placeholder\n" >"$BUNDLE/metadata/qmp.sock"
printf "not copied\n" >"$BUNDLE/disks/root.qcow2"
printf "not copied\n" >"$BUNDLE/installer.iso"

mkdir -p "$LOCAL_OUTPUT" "$SOCKET_OUTPUT"

LOCAL_STDOUT="$STORE/local-diagnostics.stdout"
bridgevm diagnostics bundle "$VM_NAME" --output "$LOCAL_OUTPUT" >"$LOCAL_STDOUT"
LOCAL_BUNDLE="$(extract_output_path <"$LOCAL_STDOUT")"
validate_bundle "$LOCAL_BUNDLE" "$LOCAL_STDOUT" "local CLI"

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

MISSING_SOCKET_OUTPUT="$STORE/missing-socket-diagnostics"
assert_fails_contains \
  "socket missing vm diagnostics" \
  "VM not found" \
  bridgevm_socket diagnostics bundle missing-vm --output "$MISSING_SOCKET_OUTPUT"
assert_no_bundle_created "socket missing vm diagnostics" "$MISSING_SOCKET_OUTPUT"

SOCKET_STDOUT="$STORE/socket-diagnostics.stdout"
bridgevm_socket diagnostics bundle "$VM_NAME" --output "$SOCKET_OUTPUT" >"$SOCKET_STDOUT"
SOCKET_BUNDLE="$(extract_output_path <"$SOCKET_STDOUT")"
validate_bundle "$SOCKET_BUNDLE" "$SOCKET_STDOUT" "socket CLI"

(
  cd "$LOCAL_BUNDLE"
  find . -type f | sed 's#^\./##' | sort
) >"$STORE/local-files.txt"
(
  cd "$SOCKET_BUNDLE"
  find . -type f | sed 's#^\./##' | sort
) >"$STORE/socket-files.txt"
diff -u "$STORE/local-files.txt" "$STORE/socket-files.txt" >/dev/null \
  || fail "local CLI and socket API diagnostic bundle file lists differed"

local_metadata_files="$STORE/local-metadata-files.txt"
socket_metadata_files="$STORE/socket-metadata-files.txt"
python3 - "$LOCAL_BUNDLE/diagnostic-bundle.json" >"$local_metadata_files" <<'PY'
import json
import sys

for item in sorted(json.load(open(sys.argv[1]))["files"]):
    print(item)
PY
python3 - "$SOCKET_BUNDLE/diagnostic-bundle.json" >"$socket_metadata_files" <<'PY'
import json
import sys

for item in sorted(json.load(open(sys.argv[1]))["files"]):
    print(item)
PY
diff -u "$local_metadata_files" "$socket_metadata_files" >/dev/null \
  || fail "local CLI and socket API diagnostic metadata file lists differed"

local_file_count="$(sed -nE 's/^Files: ([0-9]+)$/\1/p' "$LOCAL_STDOUT" | tail -n 1)"
socket_file_count="$(sed -nE 's/^Files: ([0-9]+)$/\1/p' "$SOCKET_STDOUT" | tail -n 1)"
[[ "$local_file_count" == "$socket_file_count" ]] \
  || fail "local CLI and socket API reported different file counts"

echo "PASS: diagnostics CLI/socket integration smoke ($STORE)"
