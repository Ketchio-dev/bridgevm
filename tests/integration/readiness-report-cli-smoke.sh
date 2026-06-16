#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-readiness-report.XXXXXX")"
VM_NAME="ready-report"
SOCKET_VM_NAME="socket-ready-report"
COMPAT_VM_NAME="compat-ready-report"
SOCKET_COMPAT_VM_NAME="socket-compat-ready-report"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
DISK="$BUNDLE/disks/root.qcow2"
INSTALLER="$BUNDLE/media/ubuntu.iso"
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
    *) ;;
  esac
}

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  rm -rf "$STORE"
}

trap cleanup EXIT

assert_blocked_readiness_report() {
  local output="$1"
  local vm="$2"
  local disk="$3"
  local installer="$4"
  local label="$5"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Metadata only: true" "$label"
  assert_contains "$output" "Live E2E required: true" "$label"
  assert_contains "$output" "Evidence requirements:" "$label"
  assert_contains "$output" "live-boot: required=true proven=false" "$label"
  assert_contains "$output" "console: required=true proven=false" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=false" "$label"
  assert_contains "$output" "boot-media-missing:installer-image:$installer" "$label"
  assert_contains "$output" "active-disk-missing:$disk" "$label"
  assert_contains "$output" "Pre-run launch readiness:" "$label"
  assert_contains "$output" "Launch ready: false" "$label"
  assert_contains "$output" "missing-primary-disk" "$label"
  assert_contains "$output" "missing-installer-image" "$label"
  assert_contains "$output" "launch-readiness-blocker:missing-primary-disk" "$label"
  assert_contains "$output" "launch-readiness-blocker:missing-installer-image" "$label"
  assert_not_contains "$output" "runner-metadata-missing" "$label"
  assert_contains "$output" "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started" "$label"
  assert_contains "$output" "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path" "$label"
  assert_not_contains "$output" "Launch ready: true" "$label"
}

assert_pre_run_ready_readiness_report() {
  local output="$1"
  local vm="$2"
  local label="$3"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Metadata only: true" "$label"
  assert_contains "$output" "Live E2E required: true" "$label"
  assert_contains "$output" "Runner: missing metadata" "$label"
  assert_contains "$output" "Pre-run launch readiness:" "$label"
  assert_contains "$output" "Launch ready: true" "$label"
  assert_contains "$output" "Blockers: none" "$label"
  assert_contains "$output" "Fast Mode launch readiness was evaluated from the manifest and bundle without writing runner metadata" "$label"
  assert_contains "$output" "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started" "$label"
  assert_contains "$output" "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path" "$label"
  assert_not_contains "$output" "Runner dry run: true" "$label"
  assert_not_contains "$output" "started VM" "$label"
  assert_not_contains "$output" "boot succeeded" "$label"
}

assert_ready_readiness_report() {
  local output="$1"
  local vm="$2"
  local label="$3"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Metadata only: true" "$label"
  assert_contains "$output" "Live E2E required: true" "$label"
  assert_contains "$output" "Evidence requirements:" "$label"
  assert_contains "$output" "live-boot: required=true proven=false" "$label"
  assert_contains "$output" "console: required=true proven=false" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=false" "$label"
  assert_contains "$output" "Runner dry run: true" "$label"
  assert_contains "$output" "Launch ready: true" "$label"
  assert_contains "$output" "Blockers: none" "$label"
  assert_contains "$output" "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started" "$label"
  assert_contains "$output" "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path" "$label"
  assert_not_contains "$output" "started VM" "$label"
  assert_not_contains "$output" "boot succeeded" "$label"
}

assert_compatibility_blocked_readiness_report() {
  local output="$1"
  local vm="$2"
  local disk="$3"
  local label="$4"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Mode: compatibility" "$label"
  assert_contains "$output" "Metadata only: true" "$label"
  assert_contains "$output" "Live E2E required: true" "$label"
  assert_contains "$output" "Evidence requirements:" "$label"
  assert_contains "$output" "live-boot: required=true proven=false" "$label"
  assert_contains "$output" "console: required=true proven=false" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=false" "$label"
  assert_contains "$output" "active-disk-missing:$disk" "$label"
  assert_contains "$output" "Pre-run launch readiness:" "$label"
  assert_contains "$output" "Launch ready: false" "$label"
  assert_contains "$output" "missing-primary-disk" "$label"
  assert_contains "$output" "launch-readiness-blocker:missing-primary-disk" "$label"
  assert_contains "$output" "Compatibility Mode readiness is driven by disk, runner metadata, QMP, and logs rather than Fast boot media status" "$label"
  assert_not_contains "$output" "boot-media-missing" "$label"
  assert_not_contains "$output" "runner-metadata-missing" "$label"
}

assert_compatibility_runner_readiness_report() {
  local output="$1"
  local vm="$2"
  local label="$3"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Mode: compatibility" "$label"
  assert_contains "$output" "Runner: fullvm" "$label"
  assert_contains "$output" "Runner dry run: true" "$label"
  assert_contains "$output" "Launch ready: false" "$label"
  assert_contains "$output" "missing-primary-disk" "$label"
  assert_contains "$output" "launch-readiness-blocker:missing-primary-disk" "$label"
  assert_not_contains "$output" "Pre-run launch readiness:" "$label"
  assert_not_contains "$output" "boot-media-missing" "$label"
}

assert_live_evidence_readiness_report() {
  local output="$1"
  local vm="$2"
  local evidence="$3"
  local label="$4"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Live evidence: verified ($evidence)" "$label"
  assert_contains "$output" "Live evidence backend: apple-virtualization-framework" "$label"
  assert_contains "$output" "Live evidence VM: $vm" "$label"
  assert_contains "$output" "Live evidence boot mode: linux-kernel" "$label"
  assert_contains "$output" "Live evidence disk: raw" "$label"
  assert_contains "$output" "Live evidence network: nat" "$label"
  assert_contains "$output" "Live evidence serial sentinel: required=true proven=true" "$label"
  assert_contains "$output" "Live evidence viewer/console: proven=true" "$label"
  assert_contains "$output" "Live evidence QMP: proven=false" "$label"
  assert_contains "$output" "Live evidence guest-tools effects: proven=true" "$label"
  assert_contains "$output" "live-boot: required=true proven=true" "$label"
  assert_contains "$output" "console: required=true proven=true" "$label"
  assert_contains "$output" "verified preserved opt-in Apple VZ serial boot progress evidence bundle" "$label"
  assert_contains "$output" "verified serial, graphical viewer, or QMP evidence from the preserved live bundle" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=true" "$label"
  assert_contains "$output" "verified Apple VZ live evidence bundle: $evidence" "$label"
  assert_contains "$output" "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started" "$label"
  assert_not_contains "$output" "live-evidence-invalid" "$label"
}

assert_viewer_only_live_evidence_readiness_report() {
  local output="$1"
  local vm="$2"
  local evidence="$3"
  local label="$4"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Live evidence: verified ($evidence)" "$label"
  assert_contains "$output" "Live evidence backend: apple-virtualization-framework" "$label"
  assert_contains "$output" "Live evidence VM: $vm" "$label"
  assert_contains "$output" "Live evidence serial sentinel: required=false proven=false" "$label"
  assert_contains "$output" "Live evidence viewer/console: proven=true" "$label"
  assert_contains "$output" "Live evidence QMP: proven=false" "$label"
  assert_contains "$output" "Live evidence guest-tools effects: proven=true" "$label"
  assert_contains "$output" "live-boot: required=true proven=false" "$label"
  assert_contains "$output" "console: required=true proven=true" "$label"
  assert_contains "$output" "verified preserved opt-in Apple VZ launch evidence; guest boot progress evidence is still required" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=true" "$label"
  assert_not_contains "$output" "live-evidence-invalid" "$label"
}

assert_qemu_live_evidence_readiness_report() {
  local output="$1"
  local vm="$2"
  local evidence="$3"
  local label="$4"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Mode: compatibility" "$label"
  assert_contains "$output" "Live evidence: verified ($evidence)" "$label"
  assert_contains "$output" "Live evidence backend: qemu" "$label"
  assert_contains "$output" "Live evidence VM: $vm" "$label"
  assert_contains "$output" "Live evidence boot mode: compatibility" "$label"
  assert_contains "$output" "Live evidence disk: qcow2" "$label"
  assert_contains "$output" "Live evidence network: nat" "$label"
  assert_contains "$output" "Live evidence serial sentinel: required=true proven=true" "$label"
  assert_contains "$output" "Live evidence viewer/console: proven=false" "$label"
  assert_contains "$output" "Live evidence QMP: proven=true" "$label"
  assert_contains "$output" "Live evidence guest-tools effects: proven=true" "$label"
  assert_contains "$output" "live-boot: required=true proven=true" "$label"
  assert_contains "$output" "console: required=true proven=true" "$label"
  assert_contains "$output" "verified preserved opt-in QEMU serial boot progress evidence bundle" "$label"
  assert_contains "$output" "verified serial, graphical viewer, or QMP evidence from the preserved live bundle" "$label"
  assert_contains "$output" "guest-tools-effects: required=true proven=true" "$label"
  assert_contains "$output" "verified QEMU live evidence bundle: $evidence" "$label"
  assert_not_contains "$output" "live-evidence-invalid" "$label"
}

assert_qemu_qmp_only_evidence_readiness_report() {
  local output="$1"
  local vm="$2"
  local evidence="$3"
  local label="$4"

  assert_contains "$output" "Readiness report for $vm" "$label"
  assert_contains "$output" "Mode: compatibility" "$label"
  assert_contains "$output" "Live evidence: verified ($evidence)" "$label"
  assert_contains "$output" "Live evidence backend: qemu" "$label"
  assert_contains "$output" "Live evidence VM: $vm" "$label"
  assert_contains "$output" "Live evidence serial sentinel: required=false proven=false" "$label"
  assert_contains "$output" "Live evidence viewer/console: proven=false" "$label"
  assert_contains "$output" "Live evidence QMP: proven=true" "$label"
  assert_contains "$output" "live-boot: required=true proven=false" "$label"
  assert_contains "$output" "console: required=true proven=true" "$label"
  assert_contains "$output" "verified preserved opt-in QEMU launch evidence; guest boot progress evidence is still required" "$label"
  assert_contains "$output" "verified serial, graphical viewer, or QMP evidence from the preserved live bundle" "$label"
  assert_not_contains "$output" "live-evidence-invalid" "$label"
}

make_fast_vm() {
  local runner_name="$1"
  local vm="$2"

  "$runner_name" create "$vm" \
    --os ubuntu \
    --arch arm64 \
    --mode fast \
    --boot-mode linux-installer \
    --installer-image media/ubuntu.iso >/dev/null
}

prepare_fast_metadata_inputs() {
  local vm="$1"
  local bundle="$STORE/vms/$vm.vmbridge"
  local disk="$bundle/disks/root.qcow2"
  local installer="$bundle/media/ubuntu.iso"

  mkdir -p "$(dirname "$disk")" "$(dirname "$installer")"
  : >"$disk"
  : >"$installer"
}

make_live_evidence_bundle() {
  local evidence="$1"
  local bundle="$2"
  local vm="$3"
  local kernel="$bundle/media/live-kernel"
  local disk="$bundle/disks/live-root.raw"
  local serial_log="$evidence/serial.log"
  local runner_log="$evidence/runner.log"
  local serial_log_ref="serial.log"
  local runner_log_ref="runner.log"
  local runner="$evidence/AppleVzRunner"
  local viewer_frame="$evidence/viewer-frame.png"
  local viewer_sha kernel_sha disk_sha kernel_bytes disk_bytes guest_tools_effect_sha

  mkdir -p "$evidence" "$(dirname "$kernel")" "$(dirname "$disk")"
  printf 'synthetic live kernel\n' >"$kernel"
  truncate -s 1048576 "$disk"
  kernel_sha="$(shasum -a 256 "$kernel" | awk '{print $1}')"
  disk_sha="$(shasum -a 256 "$disk" | awk '{print $1}')"
  kernel_bytes="$(wc -c <"$kernel" | tr -d ' ')"
  disk_bytes="$(wc -c <"$disk" | tr -d ' ')"
  printf 'bridgevm-live-ready\n' >"$serial_log"
  printf 'runner log\n' >"$runner_log"
  printf 'bridgevm-live-clipboard-proof\n' >"$evidence/guest-tools-effect.txt"
  guest_tools_effect_sha="$(shasum -a 256 "$evidence/guest-tools-effect.txt" | awk '{print $1}')"
  base64 --decode >"$viewer_frame" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=
EOF
  viewer_sha="$(shasum -a 256 "$viewer_frame" | awk '{print $1}')"
  printf '#!/bin/sh\n' >"$runner"
  chmod +x "$runner"
  printf '%s\n' "$runner" >"$evidence/apple-vz-runner.path"
  printf 'AppleVzRunner\n' >"$evidence/apple-vz-runner.artifact"
  shasum -a 256 "$runner" | awk '{print $1}' >"$evidence/apple-vz-runner.sha256"
  : >"$evidence/live-vz-missing-helper-opt-in.stdout"
  cat >"$evidence/live-vz-missing-helper-opt-in.stderr" <<'EOF'
real Apple VZ start requires --allow-real-vz-start
EOF
  cat >"$evidence/apple-vz-validate.output" <<'EOF'
AppleVzRunner handoff ready
VZ configuration validation: ready
Configuration plan:
Boot loader: linux-kernel
Disk attachment: disk-image-raw
Network attachment: nat
EOF
  cat >"$evidence/environment.txt" <<EOF
BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
BRIDGEVM_LIVE_VZ_KERNEL=$kernel
BRIDGEVM_LIVE_VZ_RAW_DISK=$disk
BRIDGEVM_LIVE_VZ_INITRD=
BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=console=hvc0
BRIDGEVM_LIVE_VZ_MEMORY_MIB=1024
BRIDGEVM_LIVE_VZ_CPU_COUNT=2
BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=bridgevm-live-ready
BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=1
BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=1
BRIDGEVM_LIVE_VZ_RUNNER=$runner
EOF
  cat >"$evidence/fixture-manifest.json" <<EOF
{
  "store": "$STORE",
  "source_kernel": { "exists": true, "path": "$kernel", "bytes": $kernel_bytes, "sha256": "$kernel_sha" },
  "source_raw_disk": { "exists": true, "path": "$disk", "bytes": $disk_bytes, "sha256": "$disk_sha" },
  "source_initrd": { "exists": false, "path": "", "bytes": 0, "sha256": "" },
  "bundle_kernel": { "exists": true, "path": "$kernel", "bytes": $kernel_bytes, "sha256": "$kernel_sha" },
  "bundle_raw_disk": { "exists": true, "path": "$disk", "bytes": $disk_bytes, "sha256": "$disk_sha" },
  "bundle_initrd": { "exists": false, "path": "", "bytes": 0, "sha256": "" }
}
EOF
  cat >"$evidence/apple-vz-launch.json" <<EOF
{
  "vm_name": "$vm",
  "bundle_path": "$bundle",
  "guest": { "os": "ubuntu", "arch": "arm64" },
  "boot": {
    "mode": "linux-kernel",
    "kernel": { "path": "$kernel", "exists": true },
    "kernel_command_line": "console=hvc0"
  },
  "disk": { "path": "$disk", "format": "raw", "read_only": false },
  "resources": { "memory": 1024, "cpu": 2, "balloon_device": true },
  "devices": { "network": "nat", "serial_log_path": "$serial_log_ref" },
  "logs": { "runner_log_path": "$runner_log_ref" },
  "readiness": { "ready": true, "blockers": [] }
}
EOF
  cat >"$evidence/live-vz-handoff.json" <<EOF
{
  "backend": "apple-virtualization-framework",
  "vm_name": "$vm",
  "boot_mode": "linux-kernel",
  "launch_spec_path": "$evidence/apple-vz-launch.json",
  "runner_log_path": "$runner_log_ref",
  "serial_log_path": "$serial_log_ref",
  "readiness": {
    "ready": true,
    "blockers": []
  }
}
EOF
  cat >"$evidence/guest-tools-effects.json" <<EOF
{
  "proven": true,
  "backend": "bridgevm-tools-linux",
  "command": {
    "request_id": "live-guest-tools-1",
    "capability": "clipboard",
    "status": "ok"
  },
  "effects": [
    {
      "kind": "clipboard",
      "request_id": "live-guest-tools-1",
      "ok": true,
      "expected_value": "bridgevm-live-clipboard-proof",
      "observed_value": "bridgevm-live-clipboard-proof",
      "observation": "guest acknowledged clipboard command and emitted matching CommandResult"
    },
    {
      "kind": "clipboard-artifact",
      "request_id": "live-guest-tools-1",
      "ok": true,
      "artifact": "guest-tools-effect.txt",
      "sha256": "$guest_tools_effect_sha",
      "observation": "guest preserved a command-effect artifact with matching SHA-256"
    }
  ]
}
EOF
  cat >"$evidence/viewer-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-viewer",
  "artifact": "viewer-frame.png",
  "width": 1,
  "height": 1,
  "sha256": "$viewer_sha",
  "observation": "preserved viewer frame captured from live VM"
}
EOF
  cat >"$evidence/apple-vz-live-launch.output" <<EOF
AppleVzRunner handoff ready
Launch spec diagnostics:
Kernel: $kernel (exists)
Disk: $disk (exists)
AppleVzRunner starting VM: $vm
BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=1
BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=1
AppleVzRunner VM finished: $vm
EOF
  cat >"$evidence/SUMMARY.txt" <<EOF
Apple VZ live boot opt-in smoke: passed
Serial evidence: required sentinel found: bridgevm-live-ready
Store: $STORE
Bundle: $bundle
Launch spec: $evidence/apple-vz-launch.json
Handoff JSON: $evidence/live-vz-handoff.json
Runner log: $runner_log_ref
Serial log: $serial_log_ref
Fixture manifest: $evidence/fixture-manifest.json
Environment: $evidence/environment.txt
Validation output: $evidence/apple-vz-validate.output
Live launch output: $evidence/apple-vz-live-launch.output
Stop after seconds: 1
Force stop grace seconds: 1
EOF
}

make_qemu_live_evidence_bundle() {
  local evidence="$1"
  local bundle="$2"
  local vm="$3"
  local qemu_log="$bundle/logs/qemu.log"
  local serial_log="$bundle/logs/serial.log"
  local qemu_rel="qemu.log"
  local serial_rel="serial.log"
  local qmp_transcript_rel="qmp-transcript.jsonl"
  local request_id="qemu-guest-tools-1"

  mkdir -p "$evidence" "$(dirname "$qemu_log")"
  printf 'qemu-system-x86_64 started %s\nCommand: qemu-system-x86_64 -name %s -qmp unix:%s/metadata/qmp.sock,server=on,wait=off\nQMP socket: %s/metadata/qmp.sock\nQMP status: running\n' "$vm" "$vm" "$bundle" "$bundle" >"$qemu_log"
  printf 'serial boot line\nbridgevm-qemu-ready\n' >"$serial_log"
  printf 'bridgevm-qemu-file-proof\n' >"$evidence/guest-tools-effect.txt"
  cat >"$evidence/$qmp_transcript_rel" <<'EOF'
{"QMP":{"version":{"qemu":{"major":9,"minor":0,"micro":0},"package":"bridgevm-smoke"},"capabilities":[]}}
{"execute":"qmp_capabilities"}
{"return":{}}
{"execute":"query-status"}
{"return":{"running":true,"status":"running"}}
EOF
  cp "$qemu_log" "$evidence/$qemu_rel"
  cp "$serial_log" "$evidence/$serial_rel"
  local qemu_sha serial_sha qmp_transcript_sha guest_tools_effect_sha
  qemu_sha="$(shasum -a 256 "$evidence/$qemu_rel" | awk '{print $1}')"
  serial_sha="$(shasum -a 256 "$evidence/$serial_rel" | awk '{print $1}')"
  qmp_transcript_sha="$(shasum -a 256 "$evidence/$qmp_transcript_rel" | awk '{print $1}')"
  guest_tools_effect_sha="$(shasum -a 256 "$evidence/guest-tools-effect.txt" | awk '{print $1}')"

  cat >"$evidence/guest-tools-effects.json" <<EOF
{
  "proven": true,
  "backend": "bridgevm-tools-linux",
  "command": {
    "request_id": "$request_id",
    "status": "ok"
  },
  "effects": [
    {
      "kind": "filesystem",
      "request_id": "$request_id",
      "ok": true,
      "expected_value": "bridgevm-qemu-file-proof",
      "observed_value": "bridgevm-qemu-file-proof",
      "observation": "guest wrote the requested probe file and reported success"
    },
    {
      "kind": "filesystem-artifact",
      "request_id": "$request_id",
      "ok": true,
      "artifact": "guest-tools-effect.txt",
      "sha256": "$guest_tools_effect_sha",
      "observation": "guest wrote a preserved artifact with matching SHA-256"
    }
  ]
}
EOF
  cat >"$evidence/qemu-live-evidence.json" <<EOF
{
  "proven": true,
  "backend": "qemu",
  "vm_name": "$vm",
  "boot_mode": "compatibility",
  "disk_format": "qcow2",
  "network": "nat",
  "command": ["qemu-system-x86_64", "-name", "$vm", "-qmp", "unix:$bundle/metadata/qmp.sock,server=on,wait=off"],
  "qmp": {
    "running": true,
    "status": "running",
    "socket": "$bundle/metadata/qmp.sock"
  },
  "serial_sentinel": "bridgevm-qemu-ready",
  "artifacts": {
    "qemu_log": {
      "path": "$qemu_rel",
      "sha256": "$qemu_sha"
    },
    "serial_log": {
      "path": "$serial_rel",
      "sha256": "$serial_sha"
    },
    "qmp_transcript": {
      "path": "$qmp_transcript_rel",
      "sha256": "$qmp_transcript_sha"
    }
  }
}
EOF
}

rebase_apple_live_evidence_bundle_paths() {
  local old_evidence="$1"
  local new_evidence="$2"

  perl -0pi -e "s#\Q$old_evidence\E#$new_evidence#g" \
    "$new_evidence/apple-vz-launch.json" \
    "$new_evidence/live-vz-handoff.json" \
    "$new_evidence/SUMMARY.txt"
}

make_fast_vm bridgevm "$VM_NAME"

blocked_output="$(bridgevm readiness "$VM_NAME")"
assert_blocked_readiness_report "$blocked_output" "$VM_NAME" "$DISK" "$INSTALLER" "local blocked readiness"

prepare_fast_metadata_inputs "$VM_NAME"

pre_run_ready_output="$(bridgevm readiness "$VM_NAME")"
assert_pre_run_ready_readiness_report "$pre_run_ready_output" "$VM_NAME" "local pre-run ready readiness"

run_output="$(bridgevm run "$VM_NAME")"
assert_contains "$run_output" "Dry run: true" "metadata run"
assert_contains "$run_output" "Launch ready: true" "metadata run"

ready_output="$(bridgevm readiness "$VM_NAME")"
assert_ready_readiness_report "$ready_output" "$VM_NAME" "local ready readiness"

EVIDENCE_DIR="$STORE/evidence/local-live-vz"
make_live_evidence_bundle "$EVIDENCE_DIR" "$BUNDLE" "$VM_NAME"
live_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$EVIDENCE_DIR")"
assert_live_evidence_readiness_report \
  "$live_evidence_output" \
  "$VM_NAME" \
  "$EVIDENCE_DIR" \
  "local live evidence readiness"

VIEWER_ONLY_EVIDENCE_DIR="$STORE/evidence/local-live-vz-viewer-only"
cp -R "$EVIDENCE_DIR" "$VIEWER_ONLY_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$VIEWER_ONLY_EVIDENCE_DIR"
perl -0pi -e 's/BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=bridgevm-live-ready/BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=/' \
  "$VIEWER_ONLY_EVIDENCE_DIR/environment.txt"
viewer_only_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$VIEWER_ONLY_EVIDENCE_DIR")"
assert_viewer_only_live_evidence_readiness_report \
  "$viewer_only_evidence_output" \
  "$VM_NAME" \
  "$VIEWER_ONLY_EVIDENCE_DIR" \
  "local viewer-only Apple VZ evidence readiness"

BAD_HANDOFF_EVIDENCE_DIR="$STORE/evidence/local-live-vz-bad-handoff"
cp -R "$EVIDENCE_DIR" "$BAD_HANDOFF_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$BAD_HANDOFF_EVIDENCE_DIR"
perl -0pi -e 's/"backend": "apple-virtualization-framework"/"backend": "qemu"/' "$BAD_HANDOFF_EVIDENCE_DIR/live-vz-handoff.json"
bad_handoff_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$BAD_HANDOFF_EVIDENCE_DIR")"
assert_contains "$bad_handoff_evidence_output" "live-evidence-invalid:" "local bad Apple VZ handoff evidence"
assert_contains "$bad_handoff_evidence_output" "handoff backend is not apple-virtualization-framework" "local bad Apple VZ handoff evidence"
assert_not_contains "$bad_handoff_evidence_output" "Live evidence: verified" "local bad Apple VZ handoff evidence"

BLOCKED_LAUNCH_EVIDENCE_DIR="$STORE/evidence/local-live-vz-blocked-launch"
cp -R "$EVIDENCE_DIR" "$BLOCKED_LAUNCH_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$BLOCKED_LAUNCH_EVIDENCE_DIR"
perl -0pi -e 's/"readiness": \{ "ready": true, "blockers": \[\] \}/"readiness": { "ready": false, "blockers": ["missing-kernel"] }/' "$BLOCKED_LAUNCH_EVIDENCE_DIR/apple-vz-launch.json"
blocked_launch_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$BLOCKED_LAUNCH_EVIDENCE_DIR")"
assert_contains "$blocked_launch_evidence_output" "live-evidence-invalid:" "local blocked Apple VZ launch evidence"
assert_contains "$blocked_launch_evidence_output" "launch readiness is not ready" "local blocked Apple VZ launch evidence"
assert_not_contains "$blocked_launch_evidence_output" "Live evidence: verified" "local blocked Apple VZ launch evidence"

MISSING_LAUNCH_HANDOFF_MARKER_EVIDENCE_DIR="$STORE/evidence/local-live-vz-missing-launch-handoff-marker"
cp -R "$EVIDENCE_DIR" "$MISSING_LAUNCH_HANDOFF_MARKER_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$MISSING_LAUNCH_HANDOFF_MARKER_EVIDENCE_DIR"
perl -0pi -e 's/^AppleVzRunner handoff ready\n//m' \
  "$MISSING_LAUNCH_HANDOFF_MARKER_EVIDENCE_DIR/apple-vz-live-launch.output"
missing_launch_handoff_marker_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$MISSING_LAUNCH_HANDOFF_MARKER_EVIDENCE_DIR")"
assert_contains "$missing_launch_handoff_marker_output" "live-evidence-invalid:" "local missing Apple VZ launch handoff marker evidence"
assert_contains "$missing_launch_handoff_marker_output" "apple-vz-live-launch.output missing \"AppleVzRunner handoff ready\"" "local missing Apple VZ launch handoff marker evidence"
assert_not_contains "$missing_launch_handoff_marker_output" "Live evidence: verified" "local missing Apple VZ launch handoff marker evidence"

MISSING_LAUNCH_DIAGNOSTICS_MARKER_EVIDENCE_DIR="$STORE/evidence/local-live-vz-missing-launch-diagnostics-marker"
cp -R "$EVIDENCE_DIR" "$MISSING_LAUNCH_DIAGNOSTICS_MARKER_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$MISSING_LAUNCH_DIAGNOSTICS_MARKER_EVIDENCE_DIR"
perl -0pi -e 's/^Launch spec diagnostics:\n//m' \
  "$MISSING_LAUNCH_DIAGNOSTICS_MARKER_EVIDENCE_DIR/apple-vz-live-launch.output"
missing_launch_diagnostics_marker_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$MISSING_LAUNCH_DIAGNOSTICS_MARKER_EVIDENCE_DIR")"
assert_contains "$missing_launch_diagnostics_marker_output" "live-evidence-invalid:" "local missing Apple VZ launch diagnostics marker evidence"
assert_contains "$missing_launch_diagnostics_marker_output" "apple-vz-live-launch.output missing \"Launch spec diagnostics:\"" "local missing Apple VZ launch diagnostics marker evidence"
assert_not_contains "$missing_launch_diagnostics_marker_output" "Live evidence: verified" "local missing Apple VZ launch diagnostics marker evidence"

OTHER_APPLE_EVIDENCE_DIR="$STORE/evidence/local-live-vz-other-vm"
cp -R "$EVIDENCE_DIR" "$OTHER_APPLE_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$OTHER_APPLE_EVIDENCE_DIR"
perl -0pi -e "s/\"vm_name\": \"$VM_NAME\"/\"vm_name\": \"other-apple-vm\"/g" "$OTHER_APPLE_EVIDENCE_DIR/apple-vz-launch.json" "$OTHER_APPLE_EVIDENCE_DIR/live-vz-handoff.json"
other_apple_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$OTHER_APPLE_EVIDENCE_DIR")"
assert_contains "$other_apple_evidence_output" "live-evidence-invalid:" "local mismatched Apple VZ evidence"
assert_contains "$other_apple_evidence_output" "does not match readiness VM $VM_NAME" "local mismatched Apple VZ evidence"
assert_not_contains "$other_apple_evidence_output" "Live evidence: verified" "local mismatched Apple VZ evidence"

OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR="$STORE/evidence/local-live-vz-outside-serial"
cp -R "$EVIDENCE_DIR" "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR"
outside_serial_log="$STORE/outside-apple-serial.log"
printf 'bridgevm-live-ready\n' >"$outside_serial_log"
perl -0pi -e "s#\Q$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/serial.log\E#$outside_serial_log#g" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/apple-vz-launch.json" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/live-vz-handoff.json" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/SUMMARY.txt"
perl -0pi -e "s#serial\\.log#$outside_serial_log#g" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/apple-vz-launch.json" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/live-vz-handoff.json" \
  "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR/SUMMARY.txt"
outside_serial_apple_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$OUTSIDE_SERIAL_APPLE_EVIDENCE_DIR")"
assert_contains "$outside_serial_apple_evidence_output" "live-evidence-invalid:" "local outside serial Apple VZ evidence"
assert_contains "$outside_serial_apple_evidence_output" "Apple VZ serial log path must stay inside the evidence bundle" "local outside serial Apple VZ evidence"
assert_not_contains "$outside_serial_apple_evidence_output" "Live evidence: verified" "local outside serial Apple VZ evidence"

MISMATCHED_ENV_APPLE_EVIDENCE_DIR="$STORE/evidence/local-live-vz-mismatched-env"
cp -R "$EVIDENCE_DIR" "$MISMATCHED_ENV_APPLE_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$MISMATCHED_ENV_APPLE_EVIDENCE_DIR"
perl -0pi -e 's/BRIDGEVM_LIVE_VZ_MEMORY_MIB=1024/BRIDGEVM_LIVE_VZ_MEMORY_MIB=8192/' \
  "$MISMATCHED_ENV_APPLE_EVIDENCE_DIR/environment.txt"
mismatched_env_apple_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$MISMATCHED_ENV_APPLE_EVIDENCE_DIR")"
assert_contains "$mismatched_env_apple_evidence_output" "live-evidence-invalid:" "local mismatched environment Apple VZ evidence"
assert_contains "$mismatched_env_apple_evidence_output" "environment memory does not match launch spec resources" "local mismatched environment Apple VZ evidence"
assert_not_contains "$mismatched_env_apple_evidence_output" "Live evidence: verified" "local mismatched environment Apple VZ evidence"

SYMLINK_RUNNER_APPLE_EVIDENCE_DIR="$STORE/evidence/local-live-vz-symlink-runner"
cp -R "$EVIDENCE_DIR" "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$EVIDENCE_DIR" "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR"
mv "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR/AppleVzRunner" "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR/AppleVzRunner.good"
ln -s "AppleVzRunner.good" "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR/AppleVzRunner"
symlink_runner_apple_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$SYMLINK_RUNNER_APPLE_EVIDENCE_DIR")"
assert_contains "$symlink_runner_apple_evidence_output" "live-evidence-invalid:" "local symlinked Apple VZ runner evidence"
assert_contains "$symlink_runner_apple_evidence_output" "AppleVzRunner evidence must not be a symlink" "local symlinked Apple VZ runner evidence"
assert_not_contains "$symlink_runner_apple_evidence_output" "Live evidence: verified" "local symlinked Apple VZ runner evidence"

record_without_evidence_output="$(bridgevm readiness "$VM_NAME" --record-live-evidence)"
assert_contains "$record_without_evidence_output" "live-evidence-record-error:--record-live-evidence requires --live-evidence" "local record without live evidence"
assert_not_contains "$record_without_evidence_output" "Live evidence: verified" "local record without live evidence"

clear_with_evidence_output="$(bridgevm readiness "$VM_NAME" --clear-live-evidence --live-evidence "$EVIDENCE_DIR")"
assert_contains "$clear_with_evidence_output" "live-evidence-clear-error:--clear-live-evidence cannot be combined with --live-evidence or --record-live-evidence" "local clear with live evidence"
assert_not_contains "$clear_with_evidence_output" "Live evidence: verified" "local clear with live evidence"
[[ ! -f "$BUNDLE/metadata/live-evidence.json" ]] || fail "live evidence metadata was written before record"
[[ ! -d "$BUNDLE/metadata/live-evidence" ]] || fail "live evidence directory was written before record"

PRESERVED_EVIDENCE="$BUNDLE/metadata/live-evidence/latest"
recorded_live_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$EVIDENCE_DIR" --record-live-evidence)"
assert_live_evidence_readiness_report \
  "$recorded_live_evidence_output" \
  "$VM_NAME" \
  "$PRESERVED_EVIDENCE" \
  "local recorded live evidence readiness"
assert_contains \
  "$recorded_live_evidence_output" \
  "recorded preserved live evidence bundle: $PRESERVED_EVIDENCE" \
  "local recorded live evidence readiness"
[[ -f "$PRESERVED_EVIDENCE/SUMMARY.txt" ]] || fail "recorded live evidence summary missing"
[[ -f "$PRESERVED_EVIDENCE/viewer-frame.png" ]] || fail "recorded viewer frame missing"

persisted_live_evidence_output="$(bridgevm readiness "$VM_NAME")"
assert_live_evidence_readiness_report \
  "$persisted_live_evidence_output" \
  "$VM_NAME" \
  "$PRESERVED_EVIDENCE" \
  "local persisted live evidence readiness"
assert_contains \
  "$persisted_live_evidence_output" \
  "verified Apple VZ live evidence bundle: $PRESERVED_EVIDENCE" \
  "local persisted live evidence readiness"
clear_live_evidence_output="$(bridgevm readiness "$VM_NAME" --clear-live-evidence)"
assert_contains "$clear_live_evidence_output" "cleared preserved live evidence metadata" "local clear live evidence"
assert_contains "$clear_live_evidence_output" "live-boot: required=true proven=false" "local clear live evidence"
assert_contains "$clear_live_evidence_output" "console: required=true proven=false" "local clear live evidence"
assert_not_contains "$clear_live_evidence_output" "Live evidence: verified" "local clear live evidence"
[[ ! -f "$BUNDLE/metadata/live-evidence.json" ]] || fail "live evidence metadata still exists after clear"
[[ ! -d "$BUNDLE/metadata/live-evidence" ]] || fail "live evidence directory still exists after clear"

cleared_plain_readiness_output="$(bridgevm readiness "$VM_NAME")"
assert_contains "$cleared_plain_readiness_output" "live-boot: required=true proven=false" "local cleared plain readiness"
assert_not_contains "$cleared_plain_readiness_output" "Live evidence: verified" "local cleared plain readiness"

bridgevm create "$COMPAT_VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
COMPAT_BUNDLE="$STORE/vms/$COMPAT_VM_NAME.vmbridge"
COMPAT_DISK="$COMPAT_BUNDLE/disks/root.qcow2"

compat_blocked_output="$(bridgevm readiness "$COMPAT_VM_NAME")"
assert_compatibility_blocked_readiness_report \
  "$compat_blocked_output" \
  "$COMPAT_VM_NAME" \
  "$COMPAT_DISK" \
  "local compatibility blocked readiness"

compat_run_output="$(bridgevm run "$COMPAT_VM_NAME")"
assert_contains "$compat_run_output" "Dry run: true" "local compatibility metadata run"
assert_contains "$compat_run_output" "Launch ready: false" "local compatibility metadata run"
assert_contains "$compat_run_output" "missing-primary-disk" "local compatibility metadata run"

compat_runner_output="$(bridgevm readiness "$COMPAT_VM_NAME")"
assert_compatibility_runner_readiness_report \
  "$compat_runner_output" \
  "$COMPAT_VM_NAME" \
  "local compatibility runner readiness"

QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu"
make_qemu_live_evidence_bundle "$QEMU_EVIDENCE_DIR" "$COMPAT_BUNDLE" "$COMPAT_VM_NAME"
qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$QEMU_EVIDENCE_DIR")"
assert_qemu_live_evidence_readiness_report \
  "$qemu_live_evidence_output" \
  "$COMPAT_VM_NAME" \
  "$QEMU_EVIDENCE_DIR" \
  "local QEMU live evidence readiness"

QMP_ONLY_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-qmp-only"
cp -R "$QEMU_EVIDENCE_DIR" "$QMP_ONLY_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/"serial_sentinel": "bridgevm-qemu-ready"/"serial_sentinel": ""/' \
  "$QMP_ONLY_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
qmp_only_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$QMP_ONLY_QEMU_EVIDENCE_DIR")"
assert_qemu_qmp_only_evidence_readiness_report \
  "$qmp_only_qemu_live_evidence_output" \
  "$COMPAT_VM_NAME" \
  "$QMP_ONLY_QEMU_EVIDENCE_DIR" \
  "local QMP-only QEMU evidence readiness"

OTHER_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-other-vm"
make_qemu_live_evidence_bundle "$OTHER_QEMU_EVIDENCE_DIR" "$COMPAT_BUNDLE" "other-compat-vm"
other_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$OTHER_QEMU_EVIDENCE_DIR")"
assert_contains "$other_qemu_live_evidence_output" "live-evidence-invalid:" "local mismatched QEMU evidence"
assert_contains "$other_qemu_live_evidence_output" "does not match readiness VM $COMPAT_VM_NAME" "local mismatched QEMU evidence"
assert_not_contains "$other_qemu_live_evidence_output" "Live evidence: verified" "local mismatched QEMU evidence"

fast_qemu_live_evidence_output="$(bridgevm readiness "$VM_NAME" --live-evidence "$QEMU_EVIDENCE_DIR")"
assert_contains "$fast_qemu_live_evidence_output" "live-evidence-invalid:" "local QEMU evidence against Fast Mode"
assert_contains "$fast_qemu_live_evidence_output" "QEMU live evidence cannot verify fast Mode VM $VM_NAME" "local QEMU evidence against Fast Mode"
assert_not_contains "$fast_qemu_live_evidence_output" "Live evidence: verified" "local QEMU evidence against Fast Mode"

RAW_DISK_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-raw-disk"
cp -R "$QEMU_EVIDENCE_DIR" "$RAW_DISK_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/"disk_format": "qcow2"/"disk_format": "raw"/' "$RAW_DISK_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
raw_disk_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$RAW_DISK_QEMU_EVIDENCE_DIR")"
assert_contains "$raw_disk_qemu_live_evidence_output" "live-evidence-invalid:" "local raw disk QEMU evidence"
assert_contains "$raw_disk_qemu_live_evidence_output" "disk_format is not qcow2: raw" "local raw disk QEMU evidence"
assert_not_contains "$raw_disk_qemu_live_evidence_output" "Live evidence: verified" "local raw disk QEMU evidence"

BRIDGED_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-bridged"
cp -R "$QEMU_EVIDENCE_DIR" "$BRIDGED_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/"network": "nat"/"network": "bridged"/' "$BRIDGED_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
bridged_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$BRIDGED_QEMU_EVIDENCE_DIR")"
assert_contains "$bridged_qemu_live_evidence_output" "live-evidence-invalid:" "local bridged QEMU evidence"
assert_contains "$bridged_qemu_live_evidence_output" "network is not nat: bridged" "local bridged QEMU evidence"
assert_not_contains "$bridged_qemu_live_evidence_output" "Live evidence: verified" "local bridged QEMU evidence"

COMMAND_NAME_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-command-name"
cp -R "$QEMU_EVIDENCE_DIR" "$COMMAND_NAME_QEMU_EVIDENCE_DIR"
perl -0pi -e "s/\"-name\", \"$COMPAT_VM_NAME\"/\"-name\", \"other-command-vm\"/" "$COMMAND_NAME_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
command_name_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$COMMAND_NAME_QEMU_EVIDENCE_DIR")"
assert_contains "$command_name_qemu_live_evidence_output" "live-evidence-invalid:" "local command name QEMU evidence"
assert_contains "$command_name_qemu_live_evidence_output" "command -name other-command-vm does not match vm_name $COMPAT_VM_NAME" "local command name QEMU evidence"
assert_not_contains "$command_name_qemu_live_evidence_output" "Live evidence: verified" "local command name QEMU evidence"

MISSING_QMP_COMMAND_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-missing-qmp-command"
cp -R "$QEMU_EVIDENCE_DIR" "$MISSING_QMP_COMMAND_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/"-qmp"/"-no-qmp"/' "$MISSING_QMP_COMMAND_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
missing_qmp_command_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$MISSING_QMP_COMMAND_QEMU_EVIDENCE_DIR")"
assert_contains "$missing_qmp_command_qemu_live_evidence_output" "live-evidence-invalid:" "local missing command QMP evidence"
assert_contains "$missing_qmp_command_qemu_live_evidence_output" "qemu-live-evidence.json command is missing -qmp" "local missing command QMP evidence"
assert_not_contains "$missing_qmp_command_qemu_live_evidence_output" "Live evidence: verified" "local missing command QMP evidence"

LOOSE_QMP_COMMAND_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-loose-qmp-command"
cp -R "$QEMU_EVIDENCE_DIR" "$LOOSE_QMP_COMMAND_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/server=on,wait=off/server=on,wait=off,extra=1/' "$LOOSE_QMP_COMMAND_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
loose_qmp_command_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$LOOSE_QMP_COMMAND_QEMU_EVIDENCE_DIR")"
assert_contains "$loose_qmp_command_qemu_live_evidence_output" "live-evidence-invalid:" "local loose command QMP evidence"
assert_contains "$loose_qmp_command_qemu_live_evidence_output" "does not match command -qmp" "local loose command QMP evidence"
assert_not_contains "$loose_qmp_command_qemu_live_evidence_output" "Live evidence: verified" "local loose command QMP evidence"

MISMATCHED_QMP_SOCKET_QEMU_EVIDENCE_DIR="$STORE/evidence/local-qemu-mismatched-qmp-socket"
cp -R "$QEMU_EVIDENCE_DIR" "$MISMATCHED_QMP_SOCKET_QEMU_EVIDENCE_DIR"
perl -0pi -e 's#"socket": "[^"]+"#"socket": "/tmp/bridgevm-wrong-qmp.sock"#' "$MISMATCHED_QMP_SOCKET_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
mismatched_qmp_socket_qemu_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$MISMATCHED_QMP_SOCKET_QEMU_EVIDENCE_DIR")"
assert_contains "$mismatched_qmp_socket_qemu_live_evidence_output" "live-evidence-invalid:" "local mismatched QMP socket evidence"
assert_contains "$mismatched_qmp_socket_qemu_live_evidence_output" "qmp.socket /tmp/bridgevm-wrong-qmp.sock does not match expected VM QMP socket" "local mismatched QMP socket evidence"
assert_not_contains "$mismatched_qmp_socket_qemu_live_evidence_output" "Live evidence: verified" "local mismatched QMP socket evidence"

QEMU_PRESERVED_EVIDENCE="$COMPAT_BUNDLE/metadata/live-evidence/latest"
qemu_recorded_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --live-evidence "$QEMU_EVIDENCE_DIR" --record-live-evidence)"
assert_qemu_live_evidence_readiness_report \
  "$qemu_recorded_live_evidence_output" \
  "$COMPAT_VM_NAME" \
  "$QEMU_PRESERVED_EVIDENCE" \
  "local recorded QEMU live evidence readiness"
qemu_persisted_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME")"
assert_qemu_live_evidence_readiness_report \
  "$qemu_persisted_live_evidence_output" \
  "$COMPAT_VM_NAME" \
  "$QEMU_PRESERVED_EVIDENCE" \
  "local persisted QEMU live evidence readiness"
qemu_clear_live_evidence_output="$(bridgevm readiness "$COMPAT_VM_NAME" --clear-live-evidence)"
assert_contains "$qemu_clear_live_evidence_output" "cleared preserved live evidence metadata" "local clear QEMU live evidence"
assert_contains "$qemu_clear_live_evidence_output" "live-boot: required=true proven=false" "local clear QEMU live evidence"
assert_not_contains "$qemu_clear_live_evidence_output" "Live evidence: verified" "local clear QEMU live evidence"

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

SOCKET_BUNDLE="$STORE/vms/$SOCKET_VM_NAME.vmbridge"
SOCKET_DISK="$SOCKET_BUNDLE/disks/root.qcow2"
SOCKET_INSTALLER="$SOCKET_BUNDLE/media/ubuntu.iso"

make_fast_vm bridgevm_socket "$SOCKET_VM_NAME"

socket_blocked_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME")"
assert_blocked_readiness_report \
  "$socket_blocked_output" \
  "$SOCKET_VM_NAME" \
  "$SOCKET_DISK" \
  "$SOCKET_INSTALLER" \
  "socket blocked readiness"

prepare_fast_metadata_inputs "$SOCKET_VM_NAME"

socket_pre_run_ready_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME")"
assert_pre_run_ready_readiness_report \
  "$socket_pre_run_ready_output" \
  "$SOCKET_VM_NAME" \
  "socket pre-run ready readiness"

socket_run_output="$(bridgevm_socket run "$SOCKET_VM_NAME")"
assert_contains "$socket_run_output" "Dry run: true" "socket metadata run"
assert_contains "$socket_run_output" "Launch ready: true" "socket metadata run"

socket_ready_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME")"
assert_ready_readiness_report "$socket_ready_output" "$SOCKET_VM_NAME" "socket ready readiness"

SOCKET_EVIDENCE_DIR="$STORE/evidence/socket-live-vz"
make_live_evidence_bundle "$SOCKET_EVIDENCE_DIR" "$SOCKET_BUNDLE" "$SOCKET_VM_NAME"
socket_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME" --live-evidence "$SOCKET_EVIDENCE_DIR")"
assert_live_evidence_readiness_report \
  "$socket_live_evidence_output" \
  "$SOCKET_VM_NAME" \
  "$SOCKET_EVIDENCE_DIR" \
  "socket live evidence readiness"

SOCKET_VIEWER_ONLY_EVIDENCE_DIR="$STORE/evidence/socket-live-vz-viewer-only"
cp -R "$SOCKET_EVIDENCE_DIR" "$SOCKET_VIEWER_ONLY_EVIDENCE_DIR"
rebase_apple_live_evidence_bundle_paths "$SOCKET_EVIDENCE_DIR" "$SOCKET_VIEWER_ONLY_EVIDENCE_DIR"
perl -0pi -e 's/BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=bridgevm-live-ready/BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=/' \
  "$SOCKET_VIEWER_ONLY_EVIDENCE_DIR/environment.txt"
socket_viewer_only_evidence_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME" --live-evidence "$SOCKET_VIEWER_ONLY_EVIDENCE_DIR")"
assert_viewer_only_live_evidence_readiness_report \
  "$socket_viewer_only_evidence_output" \
  "$SOCKET_VM_NAME" \
  "$SOCKET_VIEWER_ONLY_EVIDENCE_DIR" \
  "socket viewer-only Apple VZ evidence readiness"

SOCKET_PRESERVED_EVIDENCE="$SOCKET_BUNDLE/metadata/live-evidence/latest"
socket_recorded_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME" --live-evidence "$SOCKET_EVIDENCE_DIR" --record-live-evidence)"
assert_live_evidence_readiness_report \
  "$socket_recorded_live_evidence_output" \
  "$SOCKET_VM_NAME" \
  "$SOCKET_PRESERVED_EVIDENCE" \
  "socket recorded live evidence readiness"
assert_contains \
  "$socket_recorded_live_evidence_output" \
  "recorded preserved live evidence bundle: $SOCKET_PRESERVED_EVIDENCE" \
  "socket recorded live evidence readiness"

socket_persisted_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME")"
assert_live_evidence_readiness_report \
  "$socket_persisted_live_evidence_output" \
  "$SOCKET_VM_NAME" \
  "$SOCKET_PRESERVED_EVIDENCE" \
  "socket persisted live evidence readiness"

socket_clear_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_VM_NAME" --clear-live-evidence)"
assert_contains "$socket_clear_live_evidence_output" "cleared preserved live evidence metadata" "socket clear live evidence"
assert_contains "$socket_clear_live_evidence_output" "live-boot: required=true proven=false" "socket clear live evidence"
assert_not_contains "$socket_clear_live_evidence_output" "Live evidence: verified" "socket clear live evidence"
[[ ! -f "$SOCKET_BUNDLE/metadata/live-evidence.json" ]] || fail "socket live evidence metadata still exists after clear"
[[ ! -d "$SOCKET_BUNDLE/metadata/live-evidence" ]] || fail "socket live evidence directory still exists after clear"

bridgevm_socket create "$SOCKET_COMPAT_VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
SOCKET_COMPAT_BUNDLE="$STORE/vms/$SOCKET_COMPAT_VM_NAME.vmbridge"
SOCKET_COMPAT_DISK="$SOCKET_COMPAT_BUNDLE/disks/root.qcow2"

socket_compat_blocked_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME")"
assert_compatibility_blocked_readiness_report \
  "$socket_compat_blocked_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "$SOCKET_COMPAT_DISK" \
  "socket compatibility blocked readiness"

socket_compat_run_output="$(bridgevm_socket run "$SOCKET_COMPAT_VM_NAME")"
assert_contains "$socket_compat_run_output" "Dry run: true" "socket compatibility metadata run"
assert_contains "$socket_compat_run_output" "Launch ready: false" "socket compatibility metadata run"
assert_contains "$socket_compat_run_output" "missing-primary-disk" "socket compatibility metadata run"

socket_compat_runner_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME")"
assert_compatibility_runner_readiness_report \
  "$socket_compat_runner_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "socket compatibility runner readiness"

SOCKET_QEMU_EVIDENCE_DIR="$STORE/evidence/socket-qemu"
make_qemu_live_evidence_bundle "$SOCKET_QEMU_EVIDENCE_DIR" "$SOCKET_COMPAT_BUNDLE" "$SOCKET_COMPAT_VM_NAME"
socket_qemu_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME" --live-evidence "$SOCKET_QEMU_EVIDENCE_DIR")"
assert_qemu_live_evidence_readiness_report \
  "$socket_qemu_live_evidence_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "$SOCKET_QEMU_EVIDENCE_DIR" \
  "socket QEMU live evidence readiness"

SOCKET_QMP_ONLY_QEMU_EVIDENCE_DIR="$STORE/evidence/socket-qemu-qmp-only"
cp -R "$SOCKET_QEMU_EVIDENCE_DIR" "$SOCKET_QMP_ONLY_QEMU_EVIDENCE_DIR"
perl -0pi -e 's/"serial_sentinel": "bridgevm-qemu-ready"/"serial_sentinel": ""/' \
  "$SOCKET_QMP_ONLY_QEMU_EVIDENCE_DIR/qemu-live-evidence.json"
socket_qmp_only_qemu_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME" --live-evidence "$SOCKET_QMP_ONLY_QEMU_EVIDENCE_DIR")"
assert_qemu_qmp_only_evidence_readiness_report \
  "$socket_qmp_only_qemu_live_evidence_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "$SOCKET_QMP_ONLY_QEMU_EVIDENCE_DIR" \
  "socket QMP-only QEMU evidence readiness"

SOCKET_QEMU_PRESERVED_EVIDENCE="$SOCKET_COMPAT_BUNDLE/metadata/live-evidence/latest"
socket_qemu_recorded_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME" --live-evidence "$SOCKET_QEMU_EVIDENCE_DIR" --record-live-evidence)"
assert_qemu_live_evidence_readiness_report \
  "$socket_qemu_recorded_live_evidence_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "$SOCKET_QEMU_PRESERVED_EVIDENCE" \
  "socket recorded QEMU live evidence readiness"
socket_qemu_persisted_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME")"
assert_qemu_live_evidence_readiness_report \
  "$socket_qemu_persisted_live_evidence_output" \
  "$SOCKET_COMPAT_VM_NAME" \
  "$SOCKET_QEMU_PRESERVED_EVIDENCE" \
  "socket persisted QEMU live evidence readiness"
socket_qemu_clear_live_evidence_output="$(bridgevm_socket readiness "$SOCKET_COMPAT_VM_NAME" --clear-live-evidence)"
assert_contains "$socket_qemu_clear_live_evidence_output" "cleared preserved live evidence metadata" "socket clear QEMU live evidence"
assert_contains "$socket_qemu_clear_live_evidence_output" "live-boot: required=true proven=false" "socket clear QEMU live evidence"
assert_not_contains "$socket_qemu_clear_live_evidence_output" "Live evidence: verified" "socket clear QEMU live evidence"

echo "PASS: readiness report CLI smoke ($STORE)"
