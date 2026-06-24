#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-xhci-hid-boot-key.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF xHCI HID boot-key smoke: $(basename "$0")" >&2
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

assert_probe_output() {
  local output="$1"
  local label="$2"

  assert_contains "$output" "Windows 11 Arm HVF xHCI HID boot-key report probe" "$label"
  assert_contains "$output" "QEMU: not used" "$label"
  assert_contains "$output" "Apple VZ: not used" "$label"
  assert_contains "$output" "HVF: not entered" "$label"
  assert_contains "$output" "Windows boot: not claimed" "$label"
  assert_contains "$output" "Usage page: 0x07" "$label"
  assert_contains "$output" "Usage ID: 0x2c" "$label"
  assert_contains "$output" "Key report: 00 00 2c 00 00 00 00 00" "$label"
  assert_contains "$output" "Release report: 00 00 00 00 00 00 00 00" "$label"
  assert_contains "$output" "Transfer events: 2" "$label"
  assert_contains "$output" "Blockers: none" "$label"
  assert_not_contains "$output" "qemu-system" "$label"
  assert_not_contains "$output" "Windows boot: claimed" "$label"
  assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "$label"
}

bridgevm_output="$(cargo run -q -p bridgevm-cli -- hvf windows-xhci-hid-boot-key-probe 2>&1)" \
  || fail "bridgevm hvf windows-xhci-hid-boot-key-probe command failed: $bridgevm_output"
printf '%s\n' "$bridgevm_output"
assert_probe_output "$bridgevm_output" "bridgevm xHCI HID boot-key output"
assert_no_backend_launch

runner_output="$(cargo run -q -p hvf-runner -- --windows-xhci-hid-boot-key-probe 2>&1)" \
  || fail "hvf-runner --windows-xhci-hid-boot-key-probe command failed: $runner_output"
printf '%s\n' "$runner_output"
assert_probe_output "$runner_output" "hvf-runner xHCI HID boot-key output"
assert_no_backend_launch

echo "Fake backend log: empty"
echo "PASS: Windows 11 Arm no-QEMU HVF xHCI HID boot-key CLI smoke ($STORE)"
