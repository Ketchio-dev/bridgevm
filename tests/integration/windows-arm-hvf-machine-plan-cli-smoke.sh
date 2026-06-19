#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-machine-plan.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
INSTALLER="ISO/Win11_25H2_English_Arm64_v2.iso"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF machine-plan smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"

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
  esac
}

assert_not_matches() {
  local haystack="$1"
  local regex="$2"
  local label="$3"
  if printf '%s\n' "$haystack" | grep -Eq "$regex"; then
    fail "$label unexpectedly matched /$regex/; got: $haystack"
  fi
}

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
}

output="$(cargo run -q -p bridgevm-cli -- hvf machine-plan --installer "$INSTALLER" --memory-gib 8 --vcpus 6 2>&1)" \
  || fail "machine-plan command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF machine plan" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Engine: BridgeVM HVF" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Substrate: Apple Hypervisor.framework" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Installer: $INSTALLER" "Windows Arm HVF machine-plan output"
assert_contains "$output" "QEMU: not used" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Host HVF available:" "Windows Arm HVF machine-plan output"
assert_contains "$output" "IPA bits:" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Memory: 8 GiB" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Memory map:" "Windows Arm HVF machine-plan output"
assert_contains "$output" "vCPU lifecycle:" "Windows Arm HVF machine-plan output"
assert_contains "$output" "- count: 6" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Devices:" "Windows Arm HVF machine-plan output"
assert_contains "$output" "firmware UART and RTC skeletons" "Windows Arm HVF machine-plan output"
assert_contains "$output" "read-only installer media" "Windows Arm HVF machine-plan output"
assert_contains "$output" "ISO-backed reads and read-only write rejection" "Windows Arm HVF machine-plan output"
assert_contains "$output" "system boot disk" "Windows Arm HVF machine-plan output"
assert_contains "$output" "writable host-file sector write/flush/reopen persistence boundary" "Windows Arm HVF machine-plan output"
assert_contains "$output" "sparse raw GPT/ESP/MSR/Windows layout probe" "Windows Arm HVF machine-plan output"
assert_contains "$output" "firmware handoff" "Windows Arm HVF machine-plan output"
assert_contains "$output" "TPM and Secure Boot" "Windows Arm HVF machine-plan output"
assert_contains "$output" "Overall: blocked" "Windows Arm HVF machine-plan output"
assert_not_contains "$output" "qemu-system" "Windows Arm HVF machine-plan output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm HVF machine-plan output"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF machine-plan CLI metadata smoke ($STORE)"
