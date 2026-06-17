#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
  exit 0
}

[[ "${BRIDGEVM_LIVE_VZ_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1 to run the Apple VZ live boot smoke"
[[ -n "${BRIDGEVM_LIVE_VZ_KERNEL:-}" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_KERNEL to a bootable arm64 Linux kernel"
[[ -n "${BRIDGEVM_LIVE_VZ_RAW_DISK:-}" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_RAW_DISK to a disposable bootable raw disk image"
[[ -f "$BRIDGEVM_LIVE_VZ_KERNEL" ]] || \
  skip "kernel fixture does not exist: $BRIDGEVM_LIVE_VZ_KERNEL"
[[ -f "$BRIDGEVM_LIVE_VZ_RAW_DISK" ]] || \
  skip "raw disk fixture does not exist: $BRIDGEVM_LIVE_VZ_RAW_DISK"
if [[ -n "${BRIDGEVM_LIVE_VZ_INITRD:-}" && ! -f "$BRIDGEVM_LIVE_VZ_INITRD" ]]; then
  skip "initrd fixture does not exist: $BRIDGEVM_LIVE_VZ_INITRD"
fi

STORE="$(mktemp -d "/tmp/bridgevm-live-vz.XXXXXX")"
VM_NAME="live-vz-linux"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
EVIDENCE_DIR="$STORE/evidence"
KERNEL="$BUNDLE/boot/vmlinuz"
INITRD="$BUNDLE/boot/initrd"
DISK="$BUNDLE/disks/root.raw"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
EVIDENCE_LAUNCH_SPEC="$EVIDENCE_DIR/apple-vz-launch.json"
HANDOFF_JSON="$EVIDENCE_DIR/live-vz-handoff.json"
RUNNER_SHA_FILE="$EVIDENCE_DIR/apple-vz-runner.sha256"
RUNNER_LOG="$EVIDENCE_DIR/runner.log"
SERIAL_LOG="$EVIDENCE_DIR/serial.log"
RUNNER_LOG_REF="runner.log"
SERIAL_LOG_REF="serial.log"
RUNNER_ARTIFACT="$EVIDENCE_DIR/AppleVzRunner"
RUNNER_ARTIFACT_REF="AppleVzRunner"
RUNNER_BUILD_STDERR="$EVIDENCE_DIR/apple-vz-runner-build.stderr"
VALIDATE_OUTPUT_FILE="$EVIDENCE_DIR/apple-vz-validate.output"
LAUNCH_OUTPUT_FILE="$EVIDENCE_DIR/apple-vz-live-launch.output"
READINESS_RECORD_OUTPUT_FILE="$EVIDENCE_DIR/bridgevm-readiness-record.output"
SUMMARY_FILE="$EVIDENCE_DIR/SUMMARY.txt"
FIXTURE_MANIFEST="$EVIDENCE_DIR/fixture-manifest.json"
ENVIRONMENT_FILE="$EVIDENCE_DIR/environment.txt"
STOP_AFTER_SECONDS="${BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS:-30}"
FORCE_STOP_GRACE_SECONDS="${BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS:-10}"
KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-console=hvc0 root=/dev/vda rw}"
SERIAL_EXPECTED="${BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED:-}"
VIEWER_FRAME_SOURCE="${BRIDGEVM_LIVE_VZ_VIEWER_FRAME:-}"
VIEWER_FRAME_WIDTH="${BRIDGEVM_LIVE_VZ_VIEWER_FRAME_WIDTH:-}"
VIEWER_FRAME_HEIGHT="${BRIDGEVM_LIVE_VZ_VIEWER_FRAME_HEIGHT:-}"
VIEWER_FRAME_OBSERVATION="${BRIDGEVM_LIVE_VZ_VIEWER_FRAME_OBSERVATION:-preserved viewer frame captured from live VM}"
BOOT_PROGRESS_FRAME_SOURCE="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME:-}"
BOOT_PROGRESS_FRAME_WIDTH="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_WIDTH:-}"
BOOT_PROGRESS_FRAME_HEIGHT="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_HEIGHT:-}"
BOOT_PROGRESS_STAGE="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_STAGE:-guest-boot}"
BOOT_PROGRESS_MARKER="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_MARKER:-graphical boot progress visible}"
BOOT_PROGRESS_OBSERVATION="${BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_OBSERVATION:-preserved graphical frame shows guest boot progress}"
GUEST_TOOLS_EFFECTS_SOURCE="${BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON:-}"

[[ -n "$SERIAL_EXPECTED" || -n "$BOOT_PROGRESS_FRAME_SOURCE" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED or BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME to prove guest boot progress"
[[ "$STOP_AFTER_SECONDS" =~ ^[1-9][0-9]*$ ]] || \
  skip "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS must be a positive integer"
[[ "$FORCE_STOP_GRACE_SECONDS" =~ ^[1-9][0-9]*$ ]] || \
  skip "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS must be a positive integer"

require_readable_fixture() {
  local path="$1"
  local label="$2"
  [[ ! -L "$path" ]] || skip "$label fixture must not be a symlink: $path"
  [[ -r "$path" ]] || skip "$label fixture is not readable: $path"
  [[ -s "$path" ]] || skip "$label fixture is empty: $path"
}

require_readable_fixture "$BRIDGEVM_LIVE_VZ_KERNEL" "kernel"
require_readable_fixture "$BRIDGEVM_LIVE_VZ_RAW_DISK" "raw disk"
if [[ -n "${BRIDGEVM_LIVE_VZ_INITRD:-}" ]]; then
  require_readable_fixture "$BRIDGEVM_LIVE_VZ_INITRD" "initrd"
fi

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  echo "Evidence directory: $EVIDENCE_DIR" >&2
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

assert_fails_contains() {
  local label="$1"
  local expected="$2"
  shift 2

  local stdout="$EVIDENCE_DIR/$label.stdout"
  local stderr="$EVIDENCE_DIR/$label.stderr"
  if "$@" >"$stdout" 2>"$stderr"; then
    fail "$label unexpectedly succeeded"
  fi

  local output
  output="$(cat "$stdout" "$stderr")"
  assert_contains "$output" "$expected" "$label"
}

json_string() {
  printf '%s' "$1" | perl -pe 's/\\/\\\\/g; s/"/\\"/g; s/\n/\\n/g'
}

file_size() {
  wc -c <"$1" | tr -d ' '
}

file_sha256() {
  shasum -a 256 "$1" | awk '{print $1}'
}

file_json_entry() {
  local label="$1"
  local path="$2"

  if [[ -n "$path" && -f "$path" ]]; then
    cat <<EOF
    "$label": {
      "path": "$(json_string "$path")",
      "exists": true,
      "bytes": $(file_size "$path"),
      "sha256": "$(file_sha256 "$path")"
    }
EOF
  else
    cat <<EOF
    "$label": {
      "path": "$(json_string "$path")",
      "exists": false,
      "bytes": null,
      "sha256": null
    }
EOF
  fi
}

write_fixture_manifest() {
  {
    printf '{\n'
    printf '  "generated_at_utc": "%s",\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf '  "store": "%s",\n' "$(json_string "$STORE")"
    printf '  "bundle": "%s",\n' "$(json_string "$BUNDLE")"
    file_json_entry "source_kernel" "$BRIDGEVM_LIVE_VZ_KERNEL"
    printf ',\n'
    file_json_entry "source_initrd" "${BRIDGEVM_LIVE_VZ_INITRD:-}"
    printf ',\n'
    file_json_entry "source_raw_disk" "$BRIDGEVM_LIVE_VZ_RAW_DISK"
    printf ',\n'
    file_json_entry "bundle_kernel" "$KERNEL"
    printf ',\n'
    file_json_entry "bundle_initrd" "$INITRD"
    printf ',\n'
    file_json_entry "bundle_raw_disk" "$DISK"
    printf '\n}\n'
  } >"$FIXTURE_MANIFEST"
}

write_environment_evidence() {
  {
    echo "BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=${BRIDGEVM_LIVE_VZ_ALLOW_REAL_START:-}"
    echo "BRIDGEVM_LIVE_VZ_KERNEL=$BRIDGEVM_LIVE_VZ_KERNEL"
    echo "BRIDGEVM_LIVE_VZ_INITRD=${BRIDGEVM_LIVE_VZ_INITRD:-}"
    echo "BRIDGEVM_LIVE_VZ_RAW_DISK=$BRIDGEVM_LIVE_VZ_RAW_DISK"
    echo "BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=$KERNEL_CMDLINE"
    echo "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=$STOP_AFTER_SECONDS"
    echo "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=$FORCE_STOP_GRACE_SECONDS"
    echo "BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=${SERIAL_EXPECTED:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_VIEWER_FRAME=${VIEWER_FRAME_SOURCE:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_VIEWER_FRAME_WIDTH=${VIEWER_FRAME_WIDTH:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_VIEWER_FRAME_HEIGHT=${VIEWER_FRAME_HEIGHT:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME=${BOOT_PROGRESS_FRAME_SOURCE:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_WIDTH=${BOOT_PROGRESS_FRAME_WIDTH:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_HEIGHT=${BOOT_PROGRESS_FRAME_HEIGHT:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_STAGE=$BOOT_PROGRESS_STAGE"
    echo "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_MARKER=$BOOT_PROGRESS_MARKER"
    echo "BRIDGEVM_LIVE_VZ_GUEST_TOOLS_EFFECTS_JSON=${GUEST_TOOLS_EFFECTS_SOURCE:-<unset>}"
    echo "BRIDGEVM_LIVE_VZ_MEMORY_MIB=${BRIDGEVM_LIVE_VZ_MEMORY_MIB:-4096}"
    echo "BRIDGEVM_LIVE_VZ_CPU_COUNT=${BRIDGEVM_LIVE_VZ_CPU_COUNT:-2}"
    echo "BRIDGEVM_LIVE_VZ_RUNNER=${BRIDGEVM_LIVE_VZ_RUNNER:-<auto-build>}"
  } >"$ENVIRONMENT_FILE"
}

write_summary() {
  local status="$1"
  local serial_state="not checked"
  if [[ -n "$SERIAL_EXPECTED" && -f "$SERIAL_LOG" ]] && grep -Fq "$SERIAL_EXPECTED" "$SERIAL_LOG"; then
    serial_state="required sentinel found: $SERIAL_EXPECTED"
  elif [[ -n "$SERIAL_EXPECTED" ]]; then
    serial_state="required sentinel not found yet: $SERIAL_EXPECTED"
  elif [[ -s "$SERIAL_LOG" ]]; then
    serial_state="serial log captured before sentinel was configured"
  elif [[ -e "$SERIAL_LOG" ]]; then
    serial_state="serial log exists but is empty"
  fi

  {
    echo "Apple VZ live boot opt-in smoke: $status"
    echo "Generated at UTC: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "Store: $STORE"
    echo "Bundle: $BUNDLE"
    echo "Launch spec: $EVIDENCE_LAUNCH_SPEC"
    echo "Handoff JSON: $HANDOFF_JSON"
    echo "Runner log: $RUNNER_LOG_REF"
    echo "Serial log: $SERIAL_LOG_REF"
    echo "Serial evidence: $serial_state"
    echo "Stop after seconds: $STOP_AFTER_SECONDS"
    echo "Force stop grace seconds: $FORCE_STOP_GRACE_SECONDS"
    echo "Fixture manifest: $FIXTURE_MANIFEST"
    echo "Environment: $ENVIRONMENT_FILE"
    echo "Validation output: $VALIDATE_OUTPUT_FILE"
    echo "Live launch output: $LAUNCH_OUTPUT_FILE"
    echo "Readiness record output: $READINESS_RECORD_OUTPUT_FILE"
  } >"$SUMMARY_FILE"
}

write_optional_viewer_evidence() {
  [[ -n "$VIEWER_FRAME_SOURCE" ]] || return 0
  [[ -f "$VIEWER_FRAME_SOURCE" ]] || fail "viewer frame evidence does not exist: $VIEWER_FRAME_SOURCE"
  [[ "$VIEWER_FRAME_WIDTH" =~ ^[1-9][0-9]*$ ]] || fail "BRIDGEVM_LIVE_VZ_VIEWER_FRAME_WIDTH must be a positive integer"
  [[ "$VIEWER_FRAME_HEIGHT" =~ ^[1-9][0-9]*$ ]] || fail "BRIDGEVM_LIVE_VZ_VIEWER_FRAME_HEIGHT must be a positive integer"

  local artifact="viewer-frame"
  local extension="${VIEWER_FRAME_SOURCE##*.}"
  if [[ "$extension" =~ ^[A-Za-z0-9]{1,8}$ && "$extension" != "$VIEWER_FRAME_SOURCE" ]]; then
    artifact="$artifact.$extension"
  else
    artifact="$artifact.bin"
  fi
  local artifact_path="$EVIDENCE_DIR/$artifact"
  cp "$VIEWER_FRAME_SOURCE" "$artifact_path"

  cat >"$EVIDENCE_DIR/viewer-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-viewer",
  "artifact": "$(json_string "$artifact")",
  "width": $VIEWER_FRAME_WIDTH,
  "height": $VIEWER_FRAME_HEIGHT,
  "sha256": "$(file_sha256 "$artifact_path")",
  "observation": "$(json_string "$VIEWER_FRAME_OBSERVATION")"
}
EOF
}

write_optional_boot_progress_evidence() {
  [[ -n "$BOOT_PROGRESS_FRAME_SOURCE" ]] || return 0
  [[ -f "$BOOT_PROGRESS_FRAME_SOURCE" ]] || fail "boot progress frame evidence does not exist: $BOOT_PROGRESS_FRAME_SOURCE"
  [[ "$BOOT_PROGRESS_FRAME_WIDTH" =~ ^[1-9][0-9]*$ ]] || fail "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_WIDTH must be a positive integer"
  [[ "$BOOT_PROGRESS_FRAME_HEIGHT" =~ ^[1-9][0-9]*$ ]] || fail "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_FRAME_HEIGHT must be a positive integer"
  [[ -n "$BOOT_PROGRESS_STAGE" ]] || fail "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_STAGE must not be empty"
  [[ -n "$BOOT_PROGRESS_MARKER" ]] || fail "BRIDGEVM_LIVE_VZ_BOOT_PROGRESS_MARKER must not be empty"

  local artifact="boot-progress-frame"
  local extension="${BOOT_PROGRESS_FRAME_SOURCE##*.}"
  if [[ "$extension" =~ ^[A-Za-z0-9]{1,8}$ && "$extension" != "$BOOT_PROGRESS_FRAME_SOURCE" ]]; then
    artifact="$artifact.$extension"
  else
    artifact="$artifact.bin"
  fi
  local artifact_path="$EVIDENCE_DIR/$artifact"
  cp "$BOOT_PROGRESS_FRAME_SOURCE" "$artifact_path"

  cat >"$EVIDENCE_DIR/boot-progress-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-boot-progress",
  "artifact": "$(json_string "$artifact")",
  "width": $BOOT_PROGRESS_FRAME_WIDTH,
  "height": $BOOT_PROGRESS_FRAME_HEIGHT,
  "sha256": "$(file_sha256 "$artifact_path")",
  "stage": "$(json_string "$BOOT_PROGRESS_STAGE")",
  "progress_marker": "$(json_string "$BOOT_PROGRESS_MARKER")",
  "observation": "$(json_string "$BOOT_PROGRESS_OBSERVATION")"
}
EOF
}

copy_optional_guest_tools_effects_evidence() {
  [[ -n "$GUEST_TOOLS_EFFECTS_SOURCE" ]] || return 0
  [[ -f "$GUEST_TOOLS_EFFECTS_SOURCE" ]] || fail "guest-tools effects evidence does not exist: $GUEST_TOOLS_EFFECTS_SOURCE"
  cp "$GUEST_TOOLS_EFFECTS_SOURCE" "$EVIDENCE_DIR/guest-tools-effects.json"
  python3 - "$GUEST_TOOLS_EFFECTS_SOURCE" "$EVIDENCE_DIR" <<'PY'
import hashlib
import json
import shutil
import sys
from pathlib import Path

source = Path(sys.argv[1])
evidence_dir = Path(sys.argv[2])
target = evidence_dir / "guest-tools-effects.json"

def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)

try:
    data = json.loads(target.read_text(encoding="utf-8"))
except Exception as exc:
    fail(f"guest-tools effects evidence is not valid JSON: {exc}")

effects = data.get("effects")
if effects is None:
    effects = []
if not isinstance(effects, list):
    fail("guest-tools effects field must be a list")

for index, effect in enumerate(effects):
    if not isinstance(effect, dict):
        fail(f"guest-tools effect {index} must be an object")
    artifact = effect.get("artifact")
    if not isinstance(artifact, str) or not artifact:
        continue
    artifact_path = Path(artifact)
    if not artifact_path.is_absolute():
        artifact_path = source.parent / artifact_path
    if not artifact_path.is_file() or artifact_path.is_symlink():
        fail(f"guest-tools effect {index} artifact is missing or invalid: {artifact}")
    actual_sha = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
    expected_sha = effect.get("sha256")
    if isinstance(expected_sha, str) and expected_sha and expected_sha != actual_sha:
        fail(f"guest-tools effect {index} artifact sha256 does not match: {artifact}")
    suffix = artifact_path.suffix if artifact_path.suffix and len(artifact_path.suffix) <= 9 else ".bin"
    copied_name = f"guest-tools-effect-{index}{suffix}"
    copied_path = evidence_dir / copied_name
    shutil.copyfile(artifact_path, copied_path)
    effect["artifact"] = copied_name
    effect["sha256"] = actual_sha

target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

write_store_manifest_metadata() {
  local initrd_yaml=""
  if [[ -n "${BRIDGEVM_LIVE_VZ_INITRD:-}" ]]; then
    initrd_yaml="  initrdPath: boot/initrd"
  fi

  cat >"$BUNDLE/manifest.yaml" <<EOF
schemaVersion: bridgevm.io/v1
name: $VM_NAME
mode: fast
guest:
  os: debian
  arch: arm64
backend:
  engine: lightvm
  preferred: apple-vz
  fallback: qemu-hvf-restricted
  accelerator: hvf
resources:
  profile: manual
  memory: "${BRIDGEVM_LIVE_VZ_MEMORY_MIB:-4096}"
  cpu: "${BRIDGEVM_LIVE_VZ_CPU_COUNT:-2}"
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.raw
    size: 64MiB
    format: raw
    discard: true
boot:
  mode: linux-kernel
  kernelPath: boot/vmlinuz
${initrd_yaml}
  kernelCommandLine: "$(json_string "$KERNEL_CMDLINE")"
network:
  mode: nat
  hostname: $VM_NAME.bridgevm.local
  forwards: []
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
  applications: true
  windows: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
sharedFolders: []
EOF

  local now
  now="$(date +%s)"
  cat >"$BUNDLE/metadata/state.json" <<EOF
{
  "state": "stopped",
  "updated_at_unix": $now
}
EOF
  cat >"$BUNDLE/metadata/active-disk.json" <<EOF
{
  "source": "primary",
  "path": "$(json_string "$DISK")",
  "format": "raw",
  "exists": true,
  "activated_at_unix": $now
}
EOF
  printf '[]\n' >"$BUNDLE/metadata/snapshots.json"
}

normalize_evidence_log_paths() {
  perl -0pi -e "s#\Q$RUNNER_LOG\E#$RUNNER_LOG_REF#g; s#\Q$SERIAL_LOG\E#$SERIAL_LOG_REF#g" \
    "$EVIDENCE_LAUNCH_SPEC" \
    "$HANDOFF_JSON"
}

lightvm_runner() {
  cargo run --quiet -p lightvm-runner -- "$@"
}

mkdir -p "$BUNDLE/boot" "$BUNDLE/disks" "$BUNDLE/metadata" "$BUNDLE/logs" "$EVIDENCE_DIR"
cp "$BRIDGEVM_LIVE_VZ_KERNEL" "$KERNEL"
cp "$BRIDGEVM_LIVE_VZ_RAW_DISK" "$DISK"
chmod u+rw "$DISK"

INITRD_JSON='"initrd": null,'
if [[ -n "${BRIDGEVM_LIVE_VZ_INITRD:-}" ]]; then
  cp "$BRIDGEVM_LIVE_VZ_INITRD" "$INITRD"
  INITRD_JSON="\"initrd\": { \"path\": \"$(json_string "$INITRD")\", \"exists\": true },"
fi

write_store_manifest_metadata
write_environment_evidence
write_fixture_manifest

cat >"$LAUNCH_SPEC" <<EOF
{
  "vm_name": "$(json_string "$VM_NAME")",
  "bundle_path": "$(json_string "$BUNDLE")",
  "guest": {
    "os": "ubuntu",
    "arch": "arm64"
  },
  "boot": {
    "mode": "linux-kernel",
    "installer_image": null,
    "kernel": {
      "path": "$(json_string "$KERNEL")",
      "exists": true
    },
    $INITRD_JSON
    "kernel_command_line": "$(json_string "$KERNEL_CMDLINE")",
    "macos_restore_image": null
  },
  "disk": {
    "path": "$(json_string "$DISK")",
    "format": "raw",
    "read_only": false
  },
  "resources": {
    "memory": "${BRIDGEVM_LIVE_VZ_MEMORY_MIB:-4096}",
    "cpu": "${BRIDGEVM_LIVE_VZ_CPU_COUNT:-2}",
    "display_fps_cap": "60",
    "rationale": "Manual Apple VZ live boot smoke fixture.",
    "balloon_device": true
  },
  "devices": {
    "entropy_device": true,
    "network": "nat",
    "serial_log_path": "$(json_string "$SERIAL_LOG")"
  },
  "integration": {
    "clipboard": true,
    "dynamic_resolution": true,
    "shared_folders": true,
    "virtiofs": true
  },
  "logs": {
    "runner_log_path": "$(json_string "$RUNNER_LOG")"
  },
  "readiness": {
    "ready": true,
    "blockers": []
  }
}
EOF

cp "$LAUNCH_SPEC" "$EVIDENCE_LAUNCH_SPEC"
lightvm_runner --launch-spec "$EVIDENCE_LAUNCH_SPEC" --require-ready --print-handoff >"$HANDOFF_JSON"

APPLE_VZ_RUNNER_BIN="${BRIDGEVM_LIVE_VZ_RUNNER:-}"
if [[ -z "$APPLE_VZ_RUNNER_BIN" ]]; then
  APPLE_VZ_RUNNER_BIN="$(apps/macos/scripts/build-sign-apple-vz-runner.sh 2>"$RUNNER_BUILD_STDERR")" || \
    fail "could not ad-hoc sign AppleVzRunner with virtualization entitlement; see $RUNNER_BUILD_STDERR"
else
  APPLE_VZ_RUNNER_BIN="$(apps/macos/scripts/build-sign-apple-vz-runner.sh \
    --verify-only "$APPLE_VZ_RUNNER_BIN" 2>"$RUNNER_BUILD_STDERR")" || \
    fail "BRIDGEVM_LIVE_VZ_RUNNER is not signed with the virtualization entitlement; see $RUNNER_BUILD_STDERR"
fi

cp "$APPLE_VZ_RUNNER_BIN" "$RUNNER_ARTIFACT"
chmod +x "$RUNNER_ARTIFACT"
echo "$APPLE_VZ_RUNNER_BIN" >"$EVIDENCE_DIR/apple-vz-runner.path"
echo "$RUNNER_ARTIFACT_REF" >"$EVIDENCE_DIR/apple-vz-runner.artifact"
file_sha256 "$APPLE_VZ_RUNNER_BIN" >"$RUNNER_SHA_FILE"
write_summary "prepared"

if ! "$APPLE_VZ_RUNNER_BIN" \
    --handoff-json "$HANDOFF_JSON" \
    --validate-only \
    --print-config-plan \
    --validate-vz-config >"$VALIDATE_OUTPUT_FILE" 2>&1; then
  write_summary "failed"
  fail "Apple VZ validation failed; see $VALIDATE_OUTPUT_FILE"
fi
VALIDATE_OUTPUT="$(cat "$VALIDATE_OUTPUT_FILE")"
assert_contains "$VALIDATE_OUTPUT" "AppleVzRunner handoff ready" "Apple VZ validation"
assert_contains "$VALIDATE_OUTPUT" "VZ configuration validation: ready" "Apple VZ validation"
assert_contains "$VALIDATE_OUTPUT" "Configuration plan:" "Apple VZ validation"
assert_contains "$VALIDATE_OUTPUT" "Boot loader: linux-kernel" "Apple VZ validation"
assert_contains "$VALIDATE_OUTPUT" "Disk attachment: disk-image-raw" "Apple VZ validation"
assert_contains "$VALIDATE_OUTPUT" "Network attachment: nat" "Apple VZ validation"

assert_fails_contains \
  "live-vz-missing-helper-opt-in" \
  "real Apple VZ start requires --allow-real-vz-start" \
  lightvm_runner \
    --launch-spec "$LAUNCH_SPEC" \
    --require-ready \
    --launch \
    --apple-vz-runner "$APPLE_VZ_RUNNER_BIN"

if ! {
    echo "BridgeVM live launch bounds:"
    echo "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS=$STOP_AFTER_SECONDS"
    echo "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS=$FORCE_STOP_GRACE_SECONDS"
    lightvm_runner \
      --launch-spec "$EVIDENCE_LAUNCH_SPEC" \
      --require-ready \
      --launch \
      --apple-vz-runner "$APPLE_VZ_RUNNER_BIN" \
      --apple-vz-allow-real-start \
      --apple-vz-stop-after-seconds "$STOP_AFTER_SECONDS" \
      --apple-vz-force-stop-grace-seconds "$FORCE_STOP_GRACE_SECONDS"
  } >"$LAUNCH_OUTPUT_FILE" 2>&1; then
  write_summary "failed"
  fail "Apple VZ live launch failed; see $LAUNCH_OUTPUT_FILE"
fi
if [[ ! -s "$RUNNER_LOG" ]]; then
  cp "$LAUNCH_OUTPUT_FILE" "$RUNNER_LOG"
fi

if [[ -n "$SERIAL_EXPECTED" ]]; then
  [[ -f "$SERIAL_LOG" ]] || fail "serial log was not written: $SERIAL_LOG"
  serial_output="$(cat "$SERIAL_LOG")"
  assert_contains "$serial_output" "$SERIAL_EXPECTED" "Apple VZ serial log"
fi

write_optional_viewer_evidence
write_optional_boot_progress_evidence
copy_optional_guest_tools_effects_evidence
normalize_evidence_log_paths
write_summary "passed"
tests/integration/verify-apple-vz-live-evidence.sh "$EVIDENCE_DIR"

if ! cargo run --quiet -p bridgevm-cli -- \
    --store "$STORE" \
    readiness "$VM_NAME" \
    --live-evidence "$EVIDENCE_DIR" \
    --record-live-evidence >"$READINESS_RECORD_OUTPUT_FILE" 2>&1; then
  fail "BridgeVM readiness live evidence recording failed; see $READINESS_RECORD_OUTPUT_FILE"
fi
READINESS_RECORD_OUTPUT="$(cat "$READINESS_RECORD_OUTPUT_FILE")"
assert_contains "$READINESS_RECORD_OUTPUT" "Readiness report for $VM_NAME" "BridgeVM readiness record"
assert_contains "$READINESS_RECORD_OUTPUT" "Live evidence: verified" "BridgeVM readiness record"
assert_contains "$READINESS_RECORD_OUTPUT" "Live evidence backend: apple-virtualization-framework" "BridgeVM readiness record"
assert_contains "$READINESS_RECORD_OUTPUT" "live-boot: required=true proven=true" "BridgeVM readiness record"
assert_contains "$READINESS_RECORD_OUTPUT" "console: required=true proven=true" "BridgeVM readiness record"
assert_contains "$READINESS_RECORD_OUTPUT" "recorded preserved live evidence bundle" "BridgeVM readiness record"
write_summary "passed"

echo "PASS: Apple VZ live boot opt-in smoke ($STORE)"
echo "Evidence: $EVIDENCE_DIR"
echo "Summary: $SUMMARY_FILE"
