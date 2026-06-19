#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-vz-demo-stage.XXXXXX")"
FIXTURE="$STORE/fixture"
VM_NAME="try-vz-linux"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
RUNNER_METADATA="$BUNDLE/metadata/runner.json"

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

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  if ! grep -Fq "$needle" "$file"; then
    fail "$label missing '$needle' in $file"
  fi
}

mkdir -p "$FIXTURE"
printf 'fake arm64 linux kernel fixture\n' >"$FIXTURE/linux"
printf 'fake initrd fixture\n' >"$FIXTURE/initrd.gz"
truncate -s 1M "$FIXTURE/root.raw"

output="$(
  scripts/stage-vz-linux-demo-vm.sh \
    --store "$STORE" \
    --name "$VM_NAME" \
    --fixture-dir "$FIXTURE" \
    --disk 1MiB
)"

assert_contains "$output" "Apple VZ Linux VM staged: $VM_NAME" "stage output"
assert_contains "$output" "Launch ready: true" "stage output"
assert_contains "$output" "Command: lightvm-runner --launch-spec $LAUNCH_SPEC" "stage output"
assert_contains "$output" "bridgevm-cli -- --store \"$STORE\" display \"$VM_NAME\"" "stage output"
assert_not_contains "$output" "unsupported-live-boot-mode" "stage output"
assert_not_contains "$output" "unsupported-live-disk-format" "stage output"

assert_file_contains "$BUNDLE/manifest.yaml" "mode: linux-kernel" "manifest"
assert_file_contains "$BUNDLE/manifest.yaml" "kernelPath: boot/vmlinuz" "manifest"
assert_file_contains "$BUNDLE/manifest.yaml" "initrdPath: boot/initrd" "manifest"
assert_file_contains "$BUNDLE/manifest.yaml" "path: disks/root.raw" "manifest"
assert_file_contains "$BUNDLE/manifest.yaml" "format: raw" "manifest"
assert_file_contains "$BUNDLE/boot/vmlinuz" "fake arm64 linux kernel fixture" "staged kernel"
assert_file_contains "$BUNDLE/boot/initrd" "fake initrd fixture" "staged initrd"
[[ -f "$BUNDLE/disks/root.raw" ]] || fail "staged raw disk missing"
[[ "$(wc -c <"$BUNDLE/disks/root.raw" | tr -d ' ')" == "1048576" ]] || \
  fail "staged raw disk size mismatch"

assert_file_contains "$LAUNCH_SPEC" '"mode": "linux-kernel"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"format": "raw"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"ready": true' "launch spec"
assert_file_contains "$RUNNER_METADATA" '"launch_spec_path"' "runner metadata"
assert_file_contains "$RUNNER_METADATA" "\"$LAUNCH_SPEC\"" "runner metadata"
assert_file_contains "$RUNNER_METADATA" '"--launch-spec"' "runner metadata"

runner_status="$(cargo run --quiet -p bridgevm-cli -- --store "$STORE" runner-status "$VM_NAME")"
assert_contains "$runner_status" "Launch ready: true" "runner status"
assert_contains "$runner_status" "Command: lightvm-runner --launch-spec $LAUNCH_SPEC" "runner status"

echo "PASS: Apple VZ Linux demo staging smoke ($STORE)"
