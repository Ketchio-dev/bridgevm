#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-vtpm-live-evidence.XXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT

make_fixture() {
  local dir="$1"
  install -d "$dir"
  printf '%s\n' \
    'vtpm_enabled=1' \
    'firmware_code=/BridgeVM/edk2-aarch64-secure-code.fd' >"$dir/preflight.txt"
  printf '%s\n' \
    'TPM2 TIS backend: swtpm data socket /private/tmp/bridgevm/data.sock' \
    '  tpm2-tis: base=0xc000000 size=0x5000 ACPI=TPM0/MSFT0101+TPM2-log backend=swtpm ppi=shared-memory+dsm-1.3' \
    'TPM2 TIS command summary: commands=1032 success=975 errors=57 backend_failures=0 malformed_commands=0 malformed_responses=0 last_command=0x00000155 clear=0 startup=1 self_test=1 get_capability=185 pcr_read=146 pcr_extend=81 start_auth_session=186 create_primary=3 read_public=9 nv_read_public=40 get_random=5 other=375' \
    'TPM PPI shared-memory summary: reads=13 writes=0 rejected_accesses=0 memory_overwrite_requested=false' >"$dir/run.log"
  printf '%s\n' 'cleanup_status=0' >"$dir/cleanup.txt"
  printf '%s\n' 'run_status=0' >"$dir/target-stat.txt"
}

assert_rejected() {
  local dir="$1"
  local label="$2"
  if tests/integration/verify-hvf-windows-vtpm-live-evidence.sh "$dir" >/dev/null 2>&1; then
    echo "FAIL: verifier accepted $label" >&2
    exit 1
  fi
}

good="$tmp/good"
make_fixture "$good"
tests/integration/verify-hvf-windows-vtpm-live-evidence.sh "$good" >/dev/null

no_runtime="$tmp/no-runtime"
cp -R "$good" "$no_runtime"
perl -0pi -e 's/start_auth_session=186/start_auth_session=0/' "$no_runtime/run.log"
assert_rejected "$no_runtime" "a firmware-only command profile"

backend_failure="$tmp/backend-failure"
cp -R "$good" "$backend_failure"
perl -0pi -e 's/backend_failures=0/backend_failures=1/' "$backend_failure/run.log"
assert_rejected "$backend_failure" "a backend failure"

no_ppi_reads="$tmp/no-ppi-reads"
cp -R "$good" "$no_ppi_reads"
perl -0pi -e 's/reads=13/reads=0/' "$no_ppi_reads/run.log"
assert_rejected "$no_ppi_reads" "an unobserved PPI mailbox"

echo "PASS: Windows vTPM live evidence verifier smoke"
