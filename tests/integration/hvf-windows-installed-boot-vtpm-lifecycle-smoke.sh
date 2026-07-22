#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-vtpm-smoke.XXXXXX")"
cleanup_test() {
  set +e
  terminate_owned_swtpm
  cleanup_owned_swtpm_runtime
  rm -rf -- "$tmp"
}
trap cleanup_test EXIT

source scripts/run-hvf-windows-installed-boot-args.sh
source scripts/run-hvf-windows-installed-boot-runner.sh
init_installed_boot_defaults
parse_installed_boot_args \
  --vtpm-state-dir "$tmp/state" \
  --swtpm-bin "$ROOT/tests/fixtures/fake-swtpm.py" \
  --swtpm-key-stdin
validate_installed_boot_option_combinations

EVIDENCE_DIR="$tmp/evidence"
SWTPM_PID=""
SWTPM_RUNTIME_DIR=""
SWTPM_DATA_SOCKET=""
SWTPM_CONTROL_SOCKET=""
TARGET="$tmp/target.raw"
VARS="$tmp/vars.fd"
FIRMWARE_CODE="$tmp/code.fd"
install -d "$EVIDENCE_DIR"

key_hex="000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
start_owned_swtpm < <(printf '%s' "$key_hex" | xxd -r -p)
owned_pid="$SWTPM_PID"
[[ -S "$SWTPM_DATA_SOCKET" ]]
[[ -S "$SWTPM_CONTROL_SOCKET" ]]
grep -q '^swtpm_ready=true$' "$EVIDENCE_DIR/swtpm-lifecycle.txt"
[[ -f "$VTPM_STATE_DIR/fake-state.persistent" ]]
[[ "$(cat "$VTPM_STATE_DIR/fake-key.sha256")" == "630dcd2966c4336691125448bbb25b4ff412a49c732db2c8ab8c1b8581bd710d" ]]
grep -q '^state_encryption=aes-256-cbc-etm/key-fd$' "$EVIDENCE_DIR/swtpm-lifecycle.txt"

build_installed_boot_env_args >/dev/null
printf '%s\n' "${ENV_ARGS[@]}" | grep -Fqx "BRIDGEVM_SWTPM_DATA_SOCKET=$SWTPM_DATA_SOCKET"

terminate_owned_swtpm
cleanup_owned_swtpm_runtime
if kill -0 "$owned_pid" 2>/dev/null; then
  echo "FAIL: owned swtpm process survived cleanup" >&2
  exit 1
fi
[[ -f "$VTPM_STATE_DIR/fake-state.persistent" ]]

echo "PASS: installed-boot vTPM supervisor owns sockets/process and preserves state"
