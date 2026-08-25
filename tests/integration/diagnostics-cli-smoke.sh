#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; cd "$ROOT"
STORE="$(mktemp -d "/tmp/bridgevm-diag.XXXXXX")"; VM_NAME="private-vm-name"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"; LOCAL_ROOT="$STORE/local"; SOCKET_ROOT="$STORE/socket"
SECRET='forbidden-private-payload'; DAEMON_PID=""
bridgevm() { cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"; }
bridgevmd() { cargo run --quiet -p bridgevm-daemon -- --store "$STORE"; }
bridgevm_socket() { cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
cleanup() { [[ -z "$DAEMON_PID" ]] || { kill "$DAEMON_PID" 2>/dev/null || true; wait "$DAEMON_PID" 2>/dev/null || true; }; rm -rf "$STORE"; }
trap cleanup EXIT

extract_output() { sed -nE 's/^Output: (.*)$/\1/p' | tail -1; }
assert_fails_missing() {
  local output
  if output="$("$@" 2>&1)"; then fail "missing VM diagnostics unexpectedly succeeded"; fi
  [[ "$output" == *"VM not found"* ]] || fail "missing VM error was not preserved: $output"
}

validate_bundle() {
  local output="$1" stdout="$2" label="$3"
  [[ -d "$output" ]] || fail "$label output missing"
  [[ "$(basename "$output")" == bridgevm-diagnostics-* ]] || fail "$label output name is not generic"
  [[ "$(basename "$output")" != *"$VM_NAME"* ]] || fail "$label output name leaked VM name"
  expected="$STORE/$label.expected"; actual="$STORE/$label.actual"
  printf '%s\n' diagnostic-bundle.json log-summary.json record-01.json record-02.json \
    record-03.json record-05.json vm-summary.json | sort >"$expected"
  (cd "$output" && find . -type f | sed 's#^./##' | sort) >"$actual"
  diff -u "$expected" "$actual" >/dev/null || fail "$label inventory was not fixed"
  [[ "$(find "$output" -mindepth 1 -type d | wc -l | tr -d ' ')" == 0 ]] || fail "$label copied a directory"
  grep -Fq 'Diagnostic bundle for private-vm-name' "$stdout" || fail "$label local response lost VM identity"
  grep -Fq 'Files: 7' "$stdout" || fail "$label file count mismatch"
  for forbidden in "$SECRET" "$VM_NAME" '/Users/private' 'private-file.txt' 'qemu.log' \
      'root.qcow2' 'edk2-vars.fd' 'recovery-key.txt' 'clipboard.json' 'vtpm-state.bin' \
      'secrets.json' 'download-plan.json' 'guest-tools-token.json'; do
    if grep -RFq -- "$forbidden" "$output"; then fail "$label leaked $forbidden"; fi
  done
  python3 - "$output" <<'PY'
import json,pathlib,sys
root=pathlib.Path(sys.argv[1])
meta=json.loads((root/'diagnostic-bundle.json').read_text())
assert meta['vm']==meta['source']==meta['output']=='<redacted>'
assert meta['files']==['vm-summary.json','record-01.json','record-02.json','record-03.json','record-05.json','log-summary.json','diagnostic-bundle.json']
record=json.loads((root/'record-01.json').read_text())
assert record=={'limit_reached':False,'status':'running'}
summary=json.loads((root/'log-summary.json').read_text())
assert summary['regular_file_count'] >= 1 and summary['total_bytes'] > 0
PY
}

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
assert_fails_missing bridgevm diagnostics bundle missing-vm --output "$STORE/missing"
mkdir -p "$BUNDLE/metadata/vtpm" "$BUNDLE/metadata/boot-media" "$BUNDLE/disks"
printf 'clipboard=%s /Users/private/private-file.txt\n' "$SECRET" >"$BUNDLE/logs/qemu.log"
for file in secrets.json edk2-vars.fd recovery-key.txt clipboard.json guest-tools-token.json; do
  printf '%s\n' "$SECRET" >"$BUNDLE/metadata/$file"
done
printf '%s\n' "$SECRET" >"$BUNDLE/metadata/vtpm/vtpm-state.bin"
printf '%s\n' "$SECRET" >"$BUNDLE/metadata/boot-media/download-plan.json"
printf '%s\n' "$SECRET" >"$BUNDLE/disks/root.qcow2"
cat >"$BUNDLE/metadata/qmp-supervisor.json" <<JSON
{"status":"running","limit_reached":false,"path":"/Users/private/private-file.txt","message":"$SECRET","command":["cat","private-file.txt"]}
JSON
cat >"$BUNDLE/metadata/runner.json" <<JSON
{"backend":"lightvm","state":"planned","path":"/Users/private/private-file.txt","token":"$SECRET"}
JSON
cat >"$BUNDLE/metadata/apple-vz-launch.json" <<JSON
{"ready":false,"kind":"missing_boot_media","affected_path":"private-file.txt","message":"$SECRET"}
JSON
mkdir -p "$LOCAL_ROOT" "$SOCKET_ROOT"
local_stdout="$STORE/local.stdout"; bridgevm diagnostics bundle "$VM_NAME" --output "$LOCAL_ROOT" >"$local_stdout"
local_bundle="$(extract_output <"$local_stdout")"; validate_bundle "$local_bundle" "$local_stdout" local
mv "$BUNDLE/metadata/qmp-supervisor.json" "$STORE/qmp-supervisor-real.json"
ln -s "$STORE/qmp-supervisor-real.json" "$BUNDLE/metadata/qmp-supervisor.json"
if bridgevm diagnostics bundle "$VM_NAME" --output "$STORE/symlink-output" >/dev/null 2>&1; then
  fail 'allowlisted symlink unexpectedly exported'
fi
if find "$STORE/symlink-output" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
  fail 'failed symlink export left a partial bundle or staging directory'
fi
rm "$BUNDLE/metadata/qmp-supervisor.json"; mv "$STORE/qmp-supervisor-real.json" "$BUNDLE/metadata/qmp-supervisor.json"
cp "$BUNDLE/metadata/qmp-supervisor.json" "$STORE/qmp-supervisor-valid.json"
truncate -s 16777217 "$BUNDLE/metadata/qmp-supervisor.json"
if bridgevm diagnostics bundle "$VM_NAME" --output "$STORE/oversize-output" >/dev/null 2>&1; then
  fail 'oversized allowlist input unexpectedly exported'
fi
if find "$STORE/oversize-output" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
  fail 'failed oversized export left a partial bundle or staging directory'
fi
mv "$STORE/qmp-supervisor-valid.json" "$BUNDLE/metadata/qmp-supervisor.json"

SOCKET="$STORE/run/bridgevmd.sock"; bridgevmd >"$STORE/daemon.log" 2>&1 & DAEMON_PID=$!
for _ in {1..100}; do [[ -S "$SOCKET" ]] && break; sleep 0.05; done
[[ -S "$SOCKET" ]] || fail 'daemon socket was not ready'
assert_fails_missing bridgevm_socket diagnostics bundle missing-vm --output "$STORE/socket-missing"
socket_stdout="$STORE/socket.stdout"; bridgevm_socket diagnostics bundle "$VM_NAME" --output "$SOCKET_ROOT" >"$socket_stdout"
socket_bundle="$(extract_output <"$socket_stdout")"; validate_bundle "$socket_bundle" "$socket_stdout" socket
for file in log-summary.json record-01.json record-02.json record-03.json record-05.json vm-summary.json; do
  cmp "$local_bundle/$file" "$socket_bundle/$file" >/dev/null \
    || fail "local and socket $file differed"
done
echo "representative_bundle_sha256=$(find "$local_bundle" -type f -print0 | sort -z | xargs -0 shasum -a 256 | shasum -a 256 | cut -d' ' -f1)"
echo 'representative_bundle_inventory=diagnostic-bundle.json,log-summary.json,record-01.json,record-02.json,record-03.json,record-05.json,vm-summary.json'
echo 'PASS: fail-closed diagnostics allowlist (temporary store removed on exit)'
