#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-boot-disk-layout-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
DISK="$STORE/win11-arm-hvf.raw"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF boot-disk layout runner smoke: $(basename "$0")" >&2
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

output="$(cargo run -q -p hvf-runner -- --windows-boot-disk-layout-probe --disk "$DISK" --size-gib 8 --create 2>&1)" \
  || fail "hvf-runner --windows-boot-disk-layout-probe command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF boot disk layout probe" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "QEMU: not used" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "HVF: not entered" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Guest execution: not entered; sparse raw GPT/UEFI Windows target disk layout" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Disk path: $DISK" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Requested size: 8 GiB" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Disk bytes: 0x200000000" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Create requested: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Created: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Reopened for verification: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Protective MBR verified: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Primary GPT verified: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Backup GPT verified: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Partition entries verified: true" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "EFI System Partition" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Microsoft Reserved" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Windows Basic Data" "Windows Arm boot-disk layout runner output"
assert_contains "$output" "Blockers: none" "Windows Arm boot-disk layout runner output"
assert_not_contains "$output" "qemu-system" "Windows Arm boot-disk layout runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm boot-disk layout runner output"
[[ -f "$DISK" ]] || fail "expected sparse raw disk to be created"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF boot-disk layout runner smoke ($STORE)"
