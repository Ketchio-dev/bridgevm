#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-live-evidence.XXXXXX")"
EVIDENCE_DIR="$STORE/evidence"
SERIAL_LOG="$EVIDENCE_DIR/serial.log"
RUNNER_LOG="$EVIDENCE_DIR/runner.log"
RUNNER_BIN="$STORE/AppleVzRunner"
RUNNER_ARTIFACT="$EVIDENCE_DIR/AppleVzRunner"
VIEWER_FRAME="$EVIDENCE_DIR/viewer-frame.png"
GUEST_TOOLS_ARTIFACT="$EVIDENCE_DIR/guest-tools-effect-1.txt"
SOURCE_KERNEL="$STORE/source/linux"
SOURCE_RAW_DISK="$STORE/source/root.raw"
BUNDLE_KERNEL="$STORE/vms/live-vz-linux.vmbridge/boot/vmlinuz"
BUNDLE_RAW_DISK="$STORE/vms/live-vz-linux.vmbridge/disks/root.raw"

fail() {
  echo "FAIL: $*" >&2
  echo "Evidence store preserved at $STORE" >&2
  exit 1
}

json_string() {
  printf '%s' "$1" | perl -pe 's/\\/\\\\/g; s/"/\\"/g; s/\n/\\n/g'
}

file_sha() {
  shasum -a 256 "$1" | awk '{print $1}'
}

file_size() {
  wc -c <"$1" | tr -d ' '
}

expect_verifier_rejects() {
  local label="$1"
  if tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR" >/dev/null 2>&1; then
    fail "verifier accepted evidence with $label"
  fi
}

mkdir -p \
  "$EVIDENCE_DIR" \
  "$STORE/source" \
  "$STORE/vms/live-vz-linux.vmbridge/boot" \
  "$STORE/vms/live-vz-linux.vmbridge/disks" \
  "$STORE/vms/live-vz-linux.vmbridge/logs"

printf "synthetic-k\n" >"$SOURCE_KERNEL"
cp "$SOURCE_KERNEL" "$BUNDLE_KERNEL"
truncate -s 1048576 "$SOURCE_RAW_DISK"
cp "$SOURCE_RAW_DISK" "$BUNDLE_RAW_DISK"
printf "bridgevm artifact proof\n" >"$GUEST_TOOLS_ARTIFACT"

cat >"$EVIDENCE_DIR/fixture-manifest.json" <<EOF
{
  "generated_at_utc": "2026-06-15T00:00:00Z",
  "store": "$(json_string "$STORE")",
  "bundle": "$(json_string "$STORE/vms/live-vz-linux.vmbridge")",
  "source_kernel": {
    "path": "$(json_string "$SOURCE_KERNEL")",
    "exists": true,
    "bytes": $(file_size "$SOURCE_KERNEL"),
    "sha256": "$(file_sha "$SOURCE_KERNEL")"
  },
  "source_initrd": {
    "path": "",
    "exists": false,
    "bytes": null,
    "sha256": null
  },
  "source_raw_disk": {
    "path": "$(json_string "$SOURCE_RAW_DISK")",
    "exists": true,
    "bytes": $(file_size "$SOURCE_RAW_DISK"),
    "sha256": "$(file_sha "$SOURCE_RAW_DISK")"
  },
  "bundle_kernel": {
    "path": "$(json_string "$BUNDLE_KERNEL")",
    "exists": true,
    "bytes": $(file_size "$BUNDLE_KERNEL"),
    "sha256": "$(file_sha "$BUNDLE_KERNEL")"
  },
  "bundle_initrd": {
    "path": "",
    "exists": false,
    "bytes": null,
    "sha256": null
  },
  "bundle_raw_disk": {
    "path": "$(json_string "$BUNDLE_RAW_DISK")",
    "exists": true,
    "bytes": $(file_size "$BUNDLE_RAW_DISK"),
    "sha256": "$(file_sha "$BUNDLE_RAW_DISK")"
  }
}
EOF

cat >"$EVIDENCE_DIR/apple-vz-launch.json" <<EOF
{
  "vm_name": "live-vz-linux",
  "bundle_path": "$(json_string "$STORE/vms/live-vz-linux.vmbridge")",
  "guest": {
    "os": "ubuntu",
    "arch": "arm64"
  },
  "boot": {
    "mode": "linux-kernel",
    "kernel": {
      "path": "$(json_string "$BUNDLE_KERNEL")",
      "exists": true
    },
    "initrd": null,
    "kernel_command_line": "console=hvc0 root=/dev/vda rw"
  },
  "disk": {
    "path": "$(json_string "$BUNDLE_RAW_DISK")",
    "format": "raw",
    "read_only": false
  },
  "devices": {
    "network": "nat",
    "serial_log_path": "$(json_string "$SERIAL_LOG")"
  },
  "resources": {
    "memory": "4096",
    "cpu": "2",
    "display_fps_cap": "60",
    "rationale": "Synthetic verifier smoke fixture.",
    "balloon_device": true
  },
  "readiness": {
    "ready": true,
    "blockers": []
  },
  "logs": {
    "runner_log_path": "$(json_string "$RUNNER_LOG")"
  }
}
EOF

cat >"$EVIDENCE_DIR/live-vz-handoff.json" <<EOF
{
  "backend": "apple-virtualization-framework",
  "vm_name": "live-vz-linux",
  "boot_mode": "linux-kernel",
  "launch_spec_path": "$(json_string "$EVIDENCE_DIR/apple-vz-launch.json")",
  "runner_log_path": "$(json_string "$RUNNER_LOG")",
  "serial_log_path": "$(json_string "$SERIAL_LOG")",
  "readiness": {
    "ready": true,
    "blockers": []
  }
}
EOF

cat >"$EVIDENCE_DIR/guest-tools-effects.json" <<'EOF'
{
  "proven": true,
  "backend": "bridgevm-tools-linux",
  "command": {
    "request_id": "guest-tools-smoke-1",
    "capability": "clipboard",
    "status": "ok"
  },
  "effects": [
    {
      "kind": "guest-command-result",
      "request_id": "guest-tools-smoke-1",
      "ok": true,
      "expected_value": "bridgevm-guest-tools-proof",
      "observed_value": "bridgevm-guest-tools-proof",
      "observation": "guest tool command acknowledged and persisted"
    },
    {
      "kind": "guest-artifact-proof",
      "request_id": "guest-tools-smoke-1",
      "ok": true,
      "artifact": "guest-tools-effect-1.txt",
      "sha256": "GUEST_TOOLS_ARTIFACT_SHA",
      "observation": "guest tool command produced a preserved artifact"
    }
  ]
}
EOF
perl -0pi -e "s/GUEST_TOOLS_ARTIFACT_SHA/$(file_sha "$GUEST_TOOLS_ARTIFACT")/" "$EVIDENCE_DIR/guest-tools-effects.json"

base64 --decode >"$VIEWER_FRAME" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=
EOF
VIEWER_SHA="$(file_sha "$VIEWER_FRAME")"
cat >"$EVIDENCE_DIR/viewer-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-viewer",
  "artifact": "viewer-frame.png",
  "width": 1,
  "height": 1,
  "sha256": "$VIEWER_SHA",
  "observation": "preserved viewer frame captured from live VM"
}
EOF

cat >"$EVIDENCE_DIR/environment.txt" <<EOF
BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1
BRIDGEVM_LIVE_VZ_KERNEL=$SOURCE_KERNEL
BRIDGEVM_LIVE_VZ_INITRD=
BRIDGEVM_LIVE_VZ_RAW_DISK=$SOURCE_RAW_DISK
BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=console=hvc0 root=/dev/vda rw
BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=30
BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=10
BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=login:
BRIDGEVM_LIVE_VZ_MEMORY_MIB=4096
BRIDGEVM_LIVE_VZ_CPU_COUNT=2
BRIDGEVM_LIVE_VZ_RUNNER=$RUNNER_BIN
EOF

cat >"$EVIDENCE_DIR/SUMMARY.txt" <<EOF
Apple VZ live boot opt-in smoke: passed
Generated at UTC: 2026-06-15T00:00:00Z
Store: $STORE
Bundle: $STORE/vms/live-vz-linux.vmbridge
Launch spec: $EVIDENCE_DIR/apple-vz-launch.json
Handoff JSON: $EVIDENCE_DIR/live-vz-handoff.json
Runner log: $RUNNER_LOG
Serial log: $SERIAL_LOG
Serial evidence: required sentinel found: login:
Stop after seconds: 30
Force stop grace seconds: 10
Fixture manifest: $EVIDENCE_DIR/fixture-manifest.json
Environment: $EVIDENCE_DIR/environment.txt
Validation output: $EVIDENCE_DIR/apple-vz-validate.output
Live launch output: $EVIDENCE_DIR/apple-vz-live-launch.output
EOF

cat >"$EVIDENCE_DIR/apple-vz-validate.output" <<'EOF'
AppleVzRunner handoff ready
VZ configuration validation: ready
Configuration plan:
Boot loader: linux-kernel
Disk attachment: disk-image-raw
Network attachment: nat
EOF

cat >"$EVIDENCE_DIR/apple-vz-live-launch.output" <<EOF
AppleVzRunner handoff ready
BridgeVM live launch bounds:
BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=30
BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=10
VM: live-vz-linux
Backend: apple-virtualization-framework
Boot mode: linux-kernel
Launch spec diagnostics:
Kernel: $STORE/vms/live-vz-linux.vmbridge/boot/vmlinuz (declared_exists=true, actual_exists=true, size_bytes=12, signature=unknown)
Disk: $STORE/vms/live-vz-linux.vmbridge/disks/root.raw (declared_exists=true, actual_exists=true, size_bytes=1048576, signature=unknown)
AppleVzRunner starting VM: live-vz-linux
AppleVzRunner: VM start requested
AppleVzRunner: VM stop handler fired: success()
AppleVzRunner VM finished: live-vz-linux (stopped)
EOF

cat >"$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stdout" <<'EOF'
EOF

cat >"$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stderr" <<'EOF'
real Apple VZ start requires --allow-real-vz-start
EOF

cat >"$RUNNER_BIN" <<'EOF'
#!/bin/sh
echo synthetic AppleVzRunner
EOF
chmod +x "$RUNNER_BIN"
cp "$RUNNER_BIN" "$RUNNER_ARTIFACT"
chmod +x "$RUNNER_ARTIFACT"
echo "$RUNNER_BIN" >"$EVIDENCE_DIR/apple-vz-runner.path"
echo "AppleVzRunner" >"$EVIDENCE_DIR/apple-vz-runner.artifact"
file_sha "$RUNNER_ARTIFACT" >"$EVIDENCE_DIR/apple-vz-runner.sha256"
printf "login:\n" >"$SERIAL_LOG"
: >"$RUNNER_LOG"

tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR" >/dev/null

mv "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR" >/dev/null
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

bad_summary="$EVIDENCE_DIR/SUMMARY.txt"
good_summary="$EVIDENCE_DIR/SUMMARY.good.txt"

expect_summary_path_rejects() {
  local label="$1"
  local from="$2"
  local to="$3"
  cp "$good_summary" "$bad_summary"
  SUMMARY_FROM="$from" SUMMARY_TO="$to" perl -0pi -e 's/\Q$ENV{SUMMARY_FROM}\E/$ENV{SUMMARY_TO}/g' "$bad_summary"
  expect_verifier_rejects "$label"
  cp "$good_summary" "$bad_summary"
}

expect_summary_line_rejects() {
  local label="$1"
  local summary_label="$2"
  local to="$3"
  cp "$good_summary" "$bad_summary"
  SUMMARY_LABEL="$summary_label" SUMMARY_TO="$to" perl -0pi -e 's/^(\Q$ENV{SUMMARY_LABEL}\E: ).*$/${1}$ENV{SUMMARY_TO}/m' "$bad_summary"
  expect_verifier_rejects "$label"
  cp "$good_summary" "$bad_summary"
}

expect_environment_entry_rejects() {
  local label="$1"
  local key="$2"
  local to="$3"
  local environment_file="$EVIDENCE_DIR/environment.txt"
  local good_environment="$EVIDENCE_DIR/environment.good.txt"
  cp "$environment_file" "$good_environment"
  ENVIRONMENT_KEY="$key" ENVIRONMENT_TO="$to" perl -0pi -e 's/^(\Q$ENV{ENVIRONMENT_KEY}\E=).*$/${1}$ENV{ENVIRONMENT_TO}/m' "$environment_file"
  expect_verifier_rejects "$label"
  mv "$good_environment" "$environment_file"
}

cp "$bad_summary" "$good_summary"
expect_summary_line_rejects \
  "a summary with a store path that does not match the fixture manifest" \
  "Store" \
  "$STORE/wrong-store"
expect_summary_line_rejects \
  "a summary with a bundle path that does not match the launch spec" \
  "Bundle" \
  "$STORE/vms/wrong-live-vz-linux.vmbridge"
expect_summary_path_rejects \
  "a summary with a launch spec path that does not match the evidence file" \
  "$EVIDENCE_DIR/apple-vz-launch.json" \
  "$EVIDENCE_DIR/wrong-apple-vz-launch.json"
expect_summary_path_rejects \
  "a summary with a handoff JSON path that does not match the evidence file" \
  "$EVIDENCE_DIR/live-vz-handoff.json" \
  "$EVIDENCE_DIR/wrong-live-vz-handoff.json"
expect_summary_path_rejects \
  "a summary with a runner log path that does not match the launch spec" \
  "$RUNNER_LOG" \
  "$STORE/vms/live-vz-linux.vmbridge/logs/wrong-lightvm.log"
expect_summary_path_rejects \
  "a summary with a serial log path that does not match the launch spec" \
  "$SERIAL_LOG" \
  "$STORE/vms/live-vz-linux.vmbridge/logs/wrong-serial.log"
expect_summary_line_rejects \
  "a summary with a fixture manifest path that does not match the evidence file" \
  "Fixture manifest" \
  "$EVIDENCE_DIR/wrong-fixture-manifest.json"
expect_summary_line_rejects \
  "a summary with an environment path that does not match the evidence file" \
  "Environment" \
  "$EVIDENCE_DIR/wrong-environment.txt"
expect_summary_line_rejects \
  "a summary with a stop-after bound that does not match the environment" \
  "Stop after seconds" \
  "45"
expect_summary_line_rejects \
  "a summary with a force-stop grace bound that does not match the environment" \
  "Force stop grace seconds" \
  "15"
expect_summary_path_rejects \
  "a summary with a validation output path that does not match the evidence file" \
  "$EVIDENCE_DIR/apple-vz-validate.output" \
  "$EVIDENCE_DIR/wrong-apple-vz-validate.output"
expect_summary_path_rejects \
  "a summary with a live launch output path that does not match the evidence file" \
  "$EVIDENCE_DIR/apple-vz-live-launch.output" \
  "$EVIDENCE_DIR/wrong-apple-vz-live-launch.output"

printf "unexpected stdout\n" >"$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stdout"
expect_verifier_rejects "missing-helper opt-in evidence with stdout output"
: >"$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stdout"

cp "$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stderr" "$EVIDENCE_DIR/live-vz-missing-helper-opt-in.good.stderr"
: >"$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stderr"
expect_verifier_rejects "missing-helper opt-in evidence with empty stderr"
mv "$EVIDENCE_DIR/live-vz-missing-helper-opt-in.good.stderr" "$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stderr"

perl -0pi -e 's/required sentinel found: login:/required sentinel not found yet: login:/' "$bad_summary"
expect_verifier_rejects "a summary that does not prove the configured serial sentinel"
perl -0pi -e 's/required sentinel not found yet: login:/required sentinel found: login:/' "$bad_summary"

rm -f "$SERIAL_LOG"
expect_verifier_rejects "a missing serial log for the configured sentinel"
printf "login:\n" >"$SERIAL_LOG"

OUTSIDE_SERIAL_LOG="$STORE/outside-serial.log"
printf "login:\n" >"$OUTSIDE_SERIAL_LOG"
cp "$EVIDENCE_DIR/apple-vz-launch.json" "$EVIDENCE_DIR/apple-vz-launch.good.json"
cp "$EVIDENCE_DIR/live-vz-handoff.json" "$EVIDENCE_DIR/live-vz-handoff.good.json"
cp "$bad_summary" "$good_summary"
perl -0pi -e "s#\Q$SERIAL_LOG\E#$OUTSIDE_SERIAL_LOG#g" \
  "$EVIDENCE_DIR/apple-vz-launch.json" \
  "$EVIDENCE_DIR/live-vz-handoff.json" \
  "$bad_summary"
expect_verifier_rejects "a serial log path outside the evidence directory"
mv "$EVIDENCE_DIR/apple-vz-launch.good.json" "$EVIDENCE_DIR/apple-vz-launch.json"
mv "$EVIDENCE_DIR/live-vz-handoff.good.json" "$EVIDENCE_DIR/live-vz-handoff.json"
mv "$good_summary" "$bad_summary"

OUTSIDE_RUNNER_LOG="$STORE/outside-runner.log"
printf "runner log\n" >"$OUTSIDE_RUNNER_LOG"
cp "$EVIDENCE_DIR/apple-vz-launch.json" "$EVIDENCE_DIR/apple-vz-launch.good.json"
cp "$EVIDENCE_DIR/live-vz-handoff.json" "$EVIDENCE_DIR/live-vz-handoff.good.json"
cp "$bad_summary" "$good_summary"
perl -0pi -e "s#\Q$RUNNER_LOG\E#$OUTSIDE_RUNNER_LOG#g" \
  "$EVIDENCE_DIR/apple-vz-launch.json" \
  "$EVIDENCE_DIR/live-vz-handoff.json" \
  "$bad_summary"
expect_verifier_rejects "a runner log path outside the evidence directory"
mv "$EVIDENCE_DIR/apple-vz-launch.good.json" "$EVIDENCE_DIR/apple-vz-launch.json"
mv "$EVIDENCE_DIR/live-vz-handoff.good.json" "$EVIDENCE_DIR/live-vz-handoff.json"
mv "$good_summary" "$bad_summary"

rm -f "$SERIAL_LOG"
ln -s "$OUTSIDE_SERIAL_LOG" "$SERIAL_LOG"
expect_verifier_rejects "a serial log symlink that resolves outside the evidence directory"
rm -f "$SERIAL_LOG"
printf "login:\n" >"$SERIAL_LOG"

rm -f "$RUNNER_LOG"
ln -s "$OUTSIDE_RUNNER_LOG" "$RUNNER_LOG"
expect_verifier_rejects "a runner log symlink that resolves outside the evidence directory"
rm -f "$RUNNER_LOG"
: >"$RUNNER_LOG"

cp "$EVIDENCE_DIR/guest-tools-effects.json" "$EVIDENCE_DIR/guest-tools-effects.good.json"
perl -0pi -e 's/"proven": true/"proven": false/' "$EVIDENCE_DIR/guest-tools-effects.json"
expect_verifier_rejects "guest-tools effects evidence not marked proven"
mv "$EVIDENCE_DIR/guest-tools-effects.good.json" "$EVIDENCE_DIR/guest-tools-effects.json"

cp "$EVIDENCE_DIR/guest-tools-effects.json" "$EVIDENCE_DIR/guest-tools-effects.good.json"
perl -0pi -e 's/"ok": true/"ok": false/' "$EVIDENCE_DIR/guest-tools-effects.json"
expect_verifier_rejects "guest-tools effect record not ok"
mv "$EVIDENCE_DIR/guest-tools-effects.good.json" "$EVIDENCE_DIR/guest-tools-effects.json"

cp "$EVIDENCE_DIR/guest-tools-effects.json" "$EVIDENCE_DIR/guest-tools-effects.good.json"
perl -0pi -e 's/,\n      "expected_value": "bridgevm-guest-tools-proof",\n      "observed_value": "bridgevm-guest-tools-proof"//' "$EVIDENCE_DIR/guest-tools-effects.json"
expect_verifier_rejects "guest-tools effect without observable value or artifact evidence"
mv "$EVIDENCE_DIR/guest-tools-effects.good.json" "$EVIDENCE_DIR/guest-tools-effects.json"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
perl -0pi -e 's/"proven": true/"proven": false/' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence not marked proven"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

mv "$VIEWER_FRAME" "$EVIDENCE_DIR/viewer-frame.good.png"
expect_verifier_rejects "viewer evidence with a missing artifact"
mv "$EVIDENCE_DIR/viewer-frame.good.png" "$VIEWER_FRAME"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
cp "$VIEWER_FRAME" "$STORE/outside-viewer-frame.png"
perl -0pi -e 's/"artifact": "viewer-frame[.]png"/"artifact": "..\/outside-viewer-frame.png"/' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence with a traversal artifact path"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
rm -f "$EVIDENCE_DIR/viewer-frame-link.png"
ln -s "$VIEWER_FRAME" "$EVIDENCE_DIR/viewer-frame-link.png"
perl -0pi -e 's/"artifact": "viewer-frame[.]png"/"artifact": "viewer-frame-link.png"/' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence with a symlink artifact"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"
rm -f "$EVIDENCE_DIR/viewer-frame-link.png"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
VIEWER_FRAME_JSON="$(json_string "$VIEWER_FRAME")"
VIEWER_FRAME_JSON="$VIEWER_FRAME_JSON" perl -0pi -e 's#("artifact": ")[^"]+(")#$1$ENV{VIEWER_FRAME_JSON}$2#' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence with an absolute artifact path"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
printf "not a png\n" >"$VIEWER_FRAME"
VIEWER_TEXT_SHA="$(file_sha "$VIEWER_FRAME")"
perl -0pi -e "s/(\"sha256\": \")[0-9a-f]{64}/\${1}$VIEWER_TEXT_SHA/" "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence with a non-PNG artifact"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"
base64 --decode >"$VIEWER_FRAME" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=
EOF

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
perl -0pi -e 's/"width": 1/"width": 2/' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer evidence with dimensions that do not match the PNG"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

perl -0pi -e 's/BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1/BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=0/' "$EVIDENCE_DIR/environment.txt"
expect_verifier_rejects "a missing live Apple VZ opt-in environment marker"
perl -0pi -e 's/BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=0/BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1/' "$EVIDENCE_DIR/environment.txt"

expect_environment_entry_rejects \
  "an environment kernel path that does not match the fixture manifest" \
  "BRIDGEVM_LIVE_VZ_KERNEL" \
  "$STORE/source/wrong-linux"
expect_environment_entry_rejects \
  "an environment raw disk path that does not match the fixture manifest" \
  "BRIDGEVM_LIVE_VZ_RAW_DISK" \
  "$STORE/source/wrong-root.raw"
expect_environment_entry_rejects \
  "an environment kernel command line that does not match the launch spec" \
  "BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE" \
  "console=hvc0 root=/dev/vdb rw"
expect_environment_entry_rejects \
  "an environment stop-after bound that does not match the summary or launch output" \
  "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS" \
  "45"
expect_environment_entry_rejects \
  "an environment force-stop grace bound that does not match the summary or launch output" \
  "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS" \
  "15"
expect_environment_entry_rejects \
  "an environment memory size that does not match the launch spec" \
  "BRIDGEVM_LIVE_VZ_MEMORY_MIB" \
  "8192"
expect_environment_entry_rejects \
  "an environment CPU count that does not match the launch spec" \
  "BRIDGEVM_LIVE_VZ_CPU_COUNT" \
  "4"
expect_environment_entry_rejects \
  "an environment runner path that does not match the recorded AppleVzRunner path" \
  "BRIDGEVM_LIVE_VZ_RUNNER" \
  "$STORE/WrongAppleVzRunner"

perl -0pi -e 's/"readiness": \{\n    "ready": true,/"readiness": {\n    "ready": false,/' "$EVIDENCE_DIR/live-vz-handoff.json"
expect_verifier_rejects "a not-ready Apple VZ handoff"
perl -0pi -e 's/"readiness": \{\n    "ready": false,/"readiness": {\n    "ready": true,/' "$EVIDENCE_DIR/live-vz-handoff.json"

cp "$EVIDENCE_DIR/apple-vz-live-launch.output" "$EVIDENCE_DIR/apple-vz-live-launch.good.output"
perl -0pi -e 's/AppleVzRunner VM finished: live-vz-linux [(]stopped[)]\n?//' "$EVIDENCE_DIR/apple-vz-live-launch.output"
expect_verifier_rejects "live launch output without a VM finished marker"
mv "$EVIDENCE_DIR/apple-vz-live-launch.good.output" "$EVIDENCE_DIR/apple-vz-live-launch.output"

cp "$EVIDENCE_DIR/apple-vz-live-launch.output" "$EVIDENCE_DIR/apple-vz-live-launch.good.output"
perl -0pi -e 's#/boot/vmlinuz #/boot/other-vmlinuz #' "$EVIDENCE_DIR/apple-vz-live-launch.output"
expect_verifier_rejects "live launch output with a kernel path that does not match the launch spec"
mv "$EVIDENCE_DIR/apple-vz-live-launch.good.output" "$EVIDENCE_DIR/apple-vz-live-launch.output"

cp "$EVIDENCE_DIR/apple-vz-live-launch.output" "$EVIDENCE_DIR/apple-vz-live-launch.good.output"
perl -0pi -e 's#/disks/root[.]raw #/disks/other-root.raw #' "$EVIDENCE_DIR/apple-vz-live-launch.output"
expect_verifier_rejects "live launch output with a disk path that does not match the launch spec"
mv "$EVIDENCE_DIR/apple-vz-live-launch.good.output" "$EVIDENCE_DIR/apple-vz-live-launch.output"

cp "$EVIDENCE_DIR/apple-vz-validate.output" "$EVIDENCE_DIR/apple-vz-validate.good.output"
perl -0pi -e 's/Configuration plan:\n?//' "$EVIDENCE_DIR/apple-vz-validate.output"
expect_verifier_rejects "validation output without a configuration plan marker"
mv "$EVIDENCE_DIR/apple-vz-validate.good.output" "$EVIDENCE_DIR/apple-vz-validate.output"

cp "$EVIDENCE_DIR/apple-vz-validate.output" "$EVIDENCE_DIR/apple-vz-validate.good.output"
perl -0pi -e 's/Boot loader: linux-kernel/Boot loader: efi-variable-store/' "$EVIDENCE_DIR/apple-vz-validate.output"
expect_verifier_rejects "validation output with an incorrect boot loader plan marker"
mv "$EVIDENCE_DIR/apple-vz-validate.good.output" "$EVIDENCE_DIR/apple-vz-validate.output"

cp "$EVIDENCE_DIR/apple-vz-validate.output" "$EVIDENCE_DIR/apple-vz-validate.good.output"
perl -0pi -e 's/Disk attachment: disk-image-raw/Disk attachment: disk-image-qcow2/' "$EVIDENCE_DIR/apple-vz-validate.output"
expect_verifier_rejects "validation output with an incorrect disk attachment plan marker"
mv "$EVIDENCE_DIR/apple-vz-validate.good.output" "$EVIDENCE_DIR/apple-vz-validate.output"

cp "$EVIDENCE_DIR/apple-vz-validate.output" "$EVIDENCE_DIR/apple-vz-validate.good.output"
perl -0pi -e 's/Network attachment: nat/Network attachment: bridged/' "$EVIDENCE_DIR/apple-vz-validate.output"
expect_verifier_rejects "validation output with an incorrect network attachment plan marker"
mv "$EVIDENCE_DIR/apple-vz-validate.good.output" "$EVIDENCE_DIR/apple-vz-validate.output"

chmod -x "$RUNNER_ARTIFACT"
expect_verifier_rejects "a non-executable AppleVzRunner evidence artifact"
chmod +x "$RUNNER_ARTIFACT"

mv "$EVIDENCE_DIR/apple-vz-runner.artifact" "$EVIDENCE_DIR/apple-vz-runner.good.artifact"
expect_verifier_rejects "missing AppleVzRunner copied artifact pointer"
mv "$EVIDENCE_DIR/apple-vz-runner.good.artifact" "$EVIDENCE_DIR/apple-vz-runner.artifact"

printf "/tmp/AppleVzRunner\n" >"$EVIDENCE_DIR/apple-vz-runner.artifact"
expect_verifier_rejects "an absolute AppleVzRunner copied artifact pointer"
printf "AppleVzRunner\n" >"$EVIDENCE_DIR/apple-vz-runner.artifact"

printf "../AppleVzRunner\n" >"$EVIDENCE_DIR/apple-vz-runner.artifact"
expect_verifier_rejects "a traversing AppleVzRunner copied artifact pointer"
printf "AppleVzRunner\n" >"$EVIDENCE_DIR/apple-vz-runner.artifact"

mv "$RUNNER_ARTIFACT" "$EVIDENCE_DIR/AppleVzRunner.good"
ln -s "$RUNNER_BIN" "$RUNNER_ARTIFACT"
expect_verifier_rejects "a symlinked AppleVzRunner evidence artifact"
rm -f "$RUNNER_ARTIFACT"
mv "$EVIDENCE_DIR/AppleVzRunner.good" "$RUNNER_ARTIFACT"

cp "$EVIDENCE_DIR/apple-vz-runner.sha256" "$EVIDENCE_DIR/apple-vz-runner.good.sha256"
printf "0000000000000000000000000000000000000000000000000000000000000000\n" >"$EVIDENCE_DIR/apple-vz-runner.sha256"
expect_verifier_rejects "an AppleVzRunner SHA-256 that does not match the recorded evidence artifact"
mv "$EVIDENCE_DIR/apple-vz-runner.good.sha256" "$EVIDENCE_DIR/apple-vz-runner.sha256"

cp "$EVIDENCE_DIR/fixture-manifest.json" "$EVIDENCE_DIR/fixture-manifest.good.json"
perl -0pi -e 's/("bundle_raw_disk": \{\n    "path": "[^"]+",\n    "exists": true,\n    "bytes": )1048576/${1}524288/' "$EVIDENCE_DIR/fixture-manifest.json"
expect_verifier_rejects "source and bundled raw disk byte-count mismatch"
mv "$EVIDENCE_DIR/fixture-manifest.good.json" "$EVIDENCE_DIR/fixture-manifest.json"

cp "$BUNDLE_RAW_DISK" "$EVIDENCE_DIR/root.raw.good"
printf "changed" >"$BUNDLE_RAW_DISK"
expect_verifier_rejects "bundled raw disk content that does not match fixture SHA"
mv "$EVIDENCE_DIR/root.raw.good" "$BUNDLE_RAW_DISK"

cp "$EVIDENCE_DIR/fixture-manifest.json" "$EVIDENCE_DIR/fixture-manifest.good.json"
perl -0pi -e 's/("bundle_kernel": \{\n    "path": "[^"]+",\n    "exists": true,\n    "bytes": )[0-9]+/${1}999/' "$EVIDENCE_DIR/fixture-manifest.json"
expect_verifier_rejects "bundled kernel byte count that does not match the file"
mv "$EVIDENCE_DIR/fixture-manifest.good.json" "$EVIDENCE_DIR/fixture-manifest.json"

tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR" >/dev/null

echo "PASS: Apple VZ live evidence verifier smoke ($STORE)"
