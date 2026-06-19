#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-virtio-block-writable-file-backing-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
DISK="$STORE/disk.img"
SECTOR="$STORE/sector-7.bin"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in VirtIO block writable file backing smoke: $(basename "$0")" >&2
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

assert_disk_prefix() {
  local expected="$1"
  local actual
  actual="$(dd if="$DISK" bs=512 skip=7 count=1 2>/dev/null | od -An -tx1 -N8 | tr -d ' \n')"
  [[ "$actual" == "$expected" ]] || fail "disk sector prefix expected $expected, got $actual"
}

dd if=/dev/zero of="$DISK" bs=512 count=16 2>/dev/null
: >"$SECTOR"
for ((i = 0; i < 512; i++)); do
  value=$(((0xa0 + i) & 0xff))
  printf "\\$(printf '%03o' "$value")" >>"$SECTOR"
done
dd if="$SECTOR" of="$DISK" bs=512 seek=7 conv=notrunc 2>/dev/null
assert_disk_prefix "a0a1a2a3a4a5a6a7"

output="$(cargo run -q -p bridgevm-cli -- hvf virtio-block-writable-file-backing-probe --disk "$DISK" 2>&1)" \
  || fail "bridgevm hvf virtio-block-writable-file-backing-probe command failed: $output"

assert_contains "$output" "VirtIO block writable file backing probe" "VirtIO block writable file backing CLI output"
assert_contains "$output" "QEMU: not used" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Apple VZ: not used" "VirtIO block writable file backing CLI output"
assert_contains "$output" "HVF: not entered" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Disk path: $DISK" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Backing kind: host-file-writable" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Configured via MMIO: true" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Configured via MMIO bus: true" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Queue notified: true" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Queue notify value: 0x0" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Initial read data prefix: 0xa0a1a2a3a4a5a6a7" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write completed: true" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write request type: 0x1" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write sector: 0x7" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write byte offset: 0xe00" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write data bytes: 0x200" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write data prefix: 0xe0e1e2e3e4e5e6e7" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write status byte: 0x0" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write used index: 0x2" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Write used length: 0x1" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Flush completed: true" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Flush request type: 0x4" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Flush status byte: 0x0" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Flush used index: 0x3" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Flush used length: 0x1" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Persisted data prefix: 0xe0e1e2e3e4e5e6e7" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Interrupt status: 0x1" "VirtIO block writable file backing CLI output"
assert_contains "$output" "Blockers: none" "VirtIO block writable file backing CLI output"
assert_not_contains "$output" "qemu-system" "VirtIO block writable file backing CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "VirtIO block writable file backing CLI output"
assert_disk_prefix "e0e1e2e3e4e5e6e7"
assert_no_backend_launch

echo "PASS: VirtIO block writable file backing CLI metadata smoke ($STORE)"
