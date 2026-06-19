#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-ready-matrix.XXXXXX")"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""

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
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
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
  grep -Fq "$needle" "$file" || fail "$label missing '$needle' in $file"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

assert_template_listing_contract() {
  local output="$1"
  local distro="$2"
  local template_id="$3"
  local installer_rel="$4"

  assert_contains "$output" "Boot template id: $template_id" "$distro templates"
  assert_contains "$output" "Guest: $distro arm64" "$distro templates"
  assert_contains "$output" "Boot template: linux-installer" "$distro templates"
  assert_contains "$output" "Boot media: $distro arm64 installer image" "$distro templates"
  assert_contains "$output" "Installer image: $installer_rel" "$distro templates"
  assert_contains "$output" "Boot note: Place the installer image at this path inside the .vmbridge bundle, or override it with --installer-image." "$distro templates"
}

assert_template_readiness_contract() {
  local label="$1"
  local runner="$2"
  local vm="$3"
  local distro="$4"
  local template_id="$5"
  local installer_rel="$6"

  local bundle="$STORE/vms/$vm.vmbridge"
  local installer="$bundle/$installer_rel"
  local disk="$bundle/disks/root.qcow2"
  local launch_spec="$bundle/metadata/apple-vz-launch.json"
  local runner_metadata="$bundle/metadata/runner.json"

  "$runner" create "$vm" --template "$template_id" >/dev/null

  local boot_media
  boot_media="$("$runner" boot-media "$vm")"
  assert_contains "$boot_media" "VM: $vm" "$label boot-media"
  assert_contains "$boot_media" "Boot mode: linux-installer" "$label boot-media"
  assert_contains "$boot_media" "Installer image: $installer" "$label boot-media"
  assert_contains "$boot_media" "Installer image exists: false" "$label boot-media"

  local initial_readiness
  initial_readiness="$("$runner" readiness "$vm")"
  assert_contains "$initial_readiness" "Readiness report for $vm" "$label initial readiness"
  assert_contains "$initial_readiness" "Mode: fast" "$label initial readiness"
  assert_contains "$initial_readiness" "State: stopped" "$label initial readiness"
  assert_contains "$initial_readiness" "Boot media installer-image: $installer (missing)" "$label initial readiness"
  assert_contains "$initial_readiness" "Active disk: $disk" "$label initial readiness"
  assert_contains "$initial_readiness" "Active disk exists: false" "$label initial readiness"
  assert_contains "$initial_readiness" "Runner: missing metadata" "$label initial readiness"
  assert_contains "$initial_readiness" "boot-media-missing:installer-image:$installer" "$label initial readiness"
  assert_contains "$initial_readiness" "active-disk-missing:$disk" "$label initial readiness"
  assert_contains "$initial_readiness" "launch-readiness-blocker:missing-primary-disk" "$label initial readiness"
  assert_contains "$initial_readiness" "launch-readiness-blocker:missing-installer-image" "$label initial readiness"
  assert_contains "$initial_readiness" "launch-readiness-blocker:unsupported-live-boot-mode" "$label initial readiness"
  assert_contains "$initial_readiness" "launch-readiness-blocker:unsupported-live-disk-format" "$label initial readiness"
  assert_not_contains "$initial_readiness" "runner-metadata-missing" "$label initial readiness"

  local prepare
  prepare="$("$runner" prepare-run "$vm")"
  assert_contains "$prepare" "Engine: lightvm" "$label prepare-run"
  assert_contains "$prepare" "Dry run: true" "$label prepare-run"
  assert_contains "$prepare" "Launch spec: $launch_spec" "$label prepare-run"
  assert_contains "$prepare" "Launch ready: false" "$label prepare-run"
  assert_contains "$prepare" "missing-primary-disk" "$label prepare-run"
  assert_contains "$prepare" "$disk" "$label prepare-run"
  assert_contains "$prepare" "missing-installer-image" "$label prepare-run"
  assert_contains "$prepare" "$installer" "$label prepare-run"
  assert_contains "$prepare" "unsupported-live-boot-mode" "$label prepare-run"
  assert_contains "$prepare" "unsupported-live-disk-format" "$label prepare-run"
  assert_file_contains "$launch_spec" "\"os\": \"$distro\"" "$label launch spec"
  assert_file_contains "$launch_spec" "\"path\": \"$installer\"" "$label launch spec"
  assert_file_contains "$launch_spec" '"ready": false' "$label launch spec"
  assert_file_contains "$launch_spec" '"missing-primary-disk"' "$label launch spec"
  assert_file_contains "$launch_spec" '"missing-installer-image"' "$label launch spec"
  assert_file_contains "$launch_spec" '"unsupported-live-boot-mode"' "$label launch spec"
  assert_file_contains "$launch_spec" '"unsupported-live-disk-format"' "$label launch spec"
  assert_file_contains "$runner_metadata" '"engine": "lightvm"' "$label runner metadata"
  assert_file_contains "$runner_metadata" '"dry_run": true' "$label runner metadata"
  assert_file_contains "$runner_metadata" "\"launch_spec_path\": \"$launch_spec\"" "$label runner metadata"

  local prepared_readiness
  prepared_readiness="$("$runner" readiness "$vm")"
  assert_contains "$prepared_readiness" "Runner: lightvm" "$label prepared readiness"
  assert_contains "$prepared_readiness" "Runner dry run: true" "$label prepared readiness"
  assert_contains "$prepared_readiness" "Launch ready: false" "$label prepared readiness"
  assert_contains "$prepared_readiness" "missing-primary-disk" "$label prepared readiness"
  assert_contains "$prepared_readiness" "Primary disk is missing; prepare or create the disk before Fast Mode launch." "$label prepared readiness"
  assert_contains "$prepared_readiness" "missing-installer-image" "$label prepared readiness"
  assert_contains "$prepared_readiness" "Installer image is missing; import, verify, or download boot media before launch." "$label prepared readiness"
  assert_contains "$prepared_readiness" "unsupported-live-boot-mode" "$label prepared readiness"
  assert_contains "$prepared_readiness" "unsupported-live-disk-format" "$label prepared readiness"
  assert_contains "$prepared_readiness" "boot-media-missing:installer-image:$installer" "$label prepared readiness"
  assert_contains "$prepared_readiness" "active-disk-missing:$disk" "$label prepared readiness"
  assert_contains "$prepared_readiness" "launch-readiness-blocker:missing-primary-disk" "$label prepared readiness"
  assert_contains "$prepared_readiness" "launch-readiness-blocker:missing-installer-image" "$label prepared readiness"
  assert_contains "$prepared_readiness" "launch-readiness-blocker:unsupported-live-boot-mode" "$label prepared readiness"
  assert_contains "$prepared_readiness" "launch-readiness-blocker:unsupported-live-disk-format" "$label prepared readiness"
  assert_not_contains "$prepared_readiness" "runner-metadata-missing" "$label prepared readiness"
}

local_templates="$(bridgevm templates)"
for row in \
  "ubuntu|ubuntu-arm64-installer|installers/ubuntu-arm64.iso" \
  "fedora|fedora-arm64-installer|installers/fedora-arm64.iso" \
  "debian|debian-arm64-installer|installers/debian-arm64.iso"; do
  IFS="|" read -r distro template_id installer_rel <<<"$row"
  assert_template_listing_contract "$local_templates" "$distro" "$template_id" "$installer_rel"
  assert_template_readiness_contract \
    "local $distro template readiness" \
    bridgevm \
    "matrix-local-$distro" \
    "$distro" \
    "$template_id" \
    "$installer_rel"
done

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon exited before socket became ready"
  fi
  sleep 0.05
done
[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

socket_templates="$(bridgevm_socket templates)"
for row in \
  "ubuntu|ubuntu-arm64-installer|installers/ubuntu-arm64.iso" \
  "fedora|fedora-arm64-installer|installers/fedora-arm64.iso" \
  "debian|debian-arm64-installer|installers/debian-arm64.iso"; do
  IFS="|" read -r distro template_id installer_rel <<<"$row"
  assert_template_listing_contract "$socket_templates" "$distro" "$template_id" "$installer_rel"
  assert_template_readiness_contract \
    "socket $distro template readiness" \
    bridgevm_socket \
    "matrix-socket-$distro" \
    "$distro" \
    "$template_id" \
    "$installer_rel"
done

echo "PASS: Fast Mode template readiness matrix smoke ($STORE)"
