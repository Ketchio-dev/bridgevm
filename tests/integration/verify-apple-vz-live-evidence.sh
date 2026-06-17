#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <evidence-dir>" >&2
  exit 2
fi

EVIDENCE_DIR="$1"
SUMMARY_FILE="$EVIDENCE_DIR/SUMMARY.txt"
FIXTURE_MANIFEST="$EVIDENCE_DIR/fixture-manifest.json"
ENVIRONMENT_FILE="$EVIDENCE_DIR/environment.txt"
LAUNCH_SPEC="$EVIDENCE_DIR/apple-vz-launch.json"
HANDOFF_JSON="$EVIDENCE_DIR/live-vz-handoff.json"
RUNNER_PATH_FILE="$EVIDENCE_DIR/apple-vz-runner.path"
RUNNER_ARTIFACT_FILE="$EVIDENCE_DIR/apple-vz-runner.artifact"
RUNNER_SHA_FILE="$EVIDENCE_DIR/apple-vz-runner.sha256"
VALIDATE_OUTPUT="$EVIDENCE_DIR/apple-vz-validate.output"
LAUNCH_OUTPUT="$EVIDENCE_DIR/apple-vz-live-launch.output"
MISSING_OPT_IN_STDOUT="$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stdout"
MISSING_OPT_IN_STDERR="$EVIDENCE_DIR/live-vz-missing-helper-opt-in.stderr"
GUEST_TOOLS_EFFECTS="$EVIDENCE_DIR/guest-tools-effects.json"
BOOT_PROGRESS_EVIDENCE="$EVIDENCE_DIR/boot-progress-evidence.json"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_file() {
  local path="$1"
  local label="$2"
  [[ -f "$path" ]] || fail "$label missing: $path"
}

assert_contains_file() {
  local path="$1"
  local needle="$2"
  local label="$3"
  grep -Fq "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_sha256_hex() {
  local value="$1"
  local label="$2"
  [[ "$value" =~ ^[0-9a-f]{64}$ ]] || fail "$label is not a SHA-256 hex digest: $value"
}

assert_file "$SUMMARY_FILE" "summary"
assert_file "$FIXTURE_MANIFEST" "fixture manifest"
assert_file "$ENVIRONMENT_FILE" "environment"
assert_file "$LAUNCH_SPEC" "launch spec"
assert_file "$HANDOFF_JSON" "handoff JSON"
assert_file "$RUNNER_PATH_FILE" "AppleVzRunner path"
assert_file "$RUNNER_ARTIFACT_FILE" "AppleVzRunner artifact"
assert_file "$RUNNER_SHA_FILE" "AppleVzRunner SHA-256"
assert_file "$VALIDATE_OUTPUT" "validation output"
assert_file "$LAUNCH_OUTPUT" "live launch output"
assert_file "$MISSING_OPT_IN_STDOUT" "missing opt-in stdout"
assert_file "$MISSING_OPT_IN_STDERR" "missing opt-in stderr"

assert_contains_file "$SUMMARY_FILE" "Apple VZ live boot opt-in smoke: passed" "summary"
assert_contains_file "$SUMMARY_FILE" "Serial evidence:" "summary"
assert_contains_file "$VALIDATE_OUTPUT" "AppleVzRunner handoff ready" "validation output"
assert_contains_file "$VALIDATE_OUTPUT" "VZ configuration validation: ready" "validation output"
assert_contains_file "$VALIDATE_OUTPUT" "Configuration plan:" "validation output"
assert_contains_file "$VALIDATE_OUTPUT" "Boot loader: linux-kernel" "validation output"
assert_contains_file "$VALIDATE_OUTPUT" "Disk attachment: disk-image-raw" "validation output"
assert_contains_file "$VALIDATE_OUTPUT" "Network attachment: nat" "validation output"
assert_contains_file "$LAUNCH_OUTPUT" "AppleVzRunner handoff ready" "live launch output"
assert_contains_file "$LAUNCH_OUTPUT" "Launch spec diagnostics:" "live launch output"
assert_contains_file "$LAUNCH_OUTPUT" "Kernel:" "live launch output"
assert_contains_file "$LAUNCH_OUTPUT" "Disk:" "live launch output"
assert_contains_file "$LAUNCH_OUTPUT" "AppleVzRunner starting VM: live-vz-linux" "live launch output"
assert_contains_file "$LAUNCH_OUTPUT" "AppleVzRunner VM finished: live-vz-linux" "live launch output"
assert_contains_file "$ENVIRONMENT_FILE" "BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1" "environment"
assert_contains_file "$MISSING_OPT_IN_STDERR" "real Apple VZ start requires --allow-real-vz-start" "missing opt-in stderr"
[[ ! -s "$MISSING_OPT_IN_STDOUT" ]] || fail "missing opt-in stdout should be empty: $MISSING_OPT_IN_STDOUT"
[[ -s "$MISSING_OPT_IN_STDERR" ]] || fail "missing opt-in stderr should be non-empty: $MISSING_OPT_IN_STDERR"

python3 - "$FIXTURE_MANIFEST" "$LAUNCH_SPEC" "$HANDOFF_JSON" "$ENVIRONMENT_FILE" "$SUMMARY_FILE" "$LAUNCH_OUTPUT" "$VALIDATE_OUTPUT" "$GUEST_TOOLS_EFFECTS" "$BOOT_PROGRESS_EVIDENCE" <<'PY'
import json
import hashlib
import re
import sys
from pathlib import Path

manifest_path, launch_path, handoff_path, environment_path, summary_path, launch_output_path, validate_output_path, guest_tools_effects_path, boot_progress_evidence_path = map(Path, sys.argv[1:10])
evidence_dir = manifest_path.parent
viewer_evidence_path = evidence_dir / "viewer-evidence.json"

def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)

def load_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        fail(f"{path} is not valid JSON: {exc}")

def require_evidence_file(path_value, label):
    if not isinstance(path_value, str) or not path_value:
        fail(f"{label} path is missing")
    path = Path(path_value)
    check_path = path if path.is_absolute() else evidence_dir / path
    try:
        resolved_path = check_path.resolve(strict=False)
        resolved_evidence_dir = evidence_dir.resolve(strict=True)
    except Exception as exc:
        fail(f"{label} path cannot be resolved: {path_value}: {exc}")
    if resolved_path != resolved_evidence_dir and resolved_evidence_dir not in resolved_path.parents:
        fail(f"{label} path must stay inside the evidence directory: {path_value}")
    if check_path.is_symlink():
        fail(f"{label} path must not be a symlink: {path_value}")
    if not check_path.is_file():
        fail(f"{label} path is missing or not a file: {path_value}")
    return check_path

def require_relative_evidence_file(path_value, label):
    if not isinstance(path_value, str) or not path_value:
        fail(f"{label} path is missing")
    path = Path(path_value)
    if path.is_absolute() or ".." in path.parts:
        fail(f"{label} path must be relative and stay inside the evidence directory: {path_value}")
    return require_evidence_file(path_value, label)

manifest = load_json(manifest_path)
launch = load_json(launch_path)
handoff = load_json(handoff_path)
environment = environment_path.read_text(encoding="utf-8")
summary = summary_path.read_text(encoding="utf-8")
launch_output = launch_output_path.read_text(encoding="utf-8", errors="replace")

environment_values = {}
for line in environment.splitlines():
    if "=" not in line:
        continue
    key, value = line.split("=", 1)
    environment_values[key] = value

required_manifest_entries = [
    "source_kernel",
    "source_raw_disk",
    "bundle_kernel",
    "bundle_raw_disk",
]
optional_manifest_entries = ["source_initrd", "bundle_initrd"]
sha_pattern = re.compile(r"^[0-9a-f]{64}$")

def verify_fixture_file(entry, key):
    path = Path(entry["path"])
    if not path.is_file():
        fail(f"fixture manifest entry path is not a file: {key}")
    if path.is_symlink():
        fail(f"fixture manifest entry path must not be a symlink: {key}")
    actual_bytes = path.stat().st_size
    if actual_bytes != entry["bytes"]:
        fail(f"fixture manifest entry byte count does not match file: {key}")
    actual_sha256 = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual_sha256 != entry["sha256"]:
        fail(f"fixture manifest entry SHA-256 does not match file: {key}")

def png_dimensions(bytes_value):
    if len(bytes_value) < 24 or bytes_value[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    if bytes_value[12:16] != b"IHDR":
        return None
    width = int.from_bytes(bytes_value[16:20], "big")
    height = int.from_bytes(bytes_value[20:24], "big")
    if width <= 0 or height <= 0:
        return None
    return width, height

for key in required_manifest_entries:
    entry = manifest.get(key)
    if not isinstance(entry, dict):
        fail(f"fixture manifest missing object entry: {key}")
    if entry.get("exists") is not True:
        fail(f"fixture manifest entry is not marked existing: {key}")
    if not isinstance(entry.get("path"), str) or not entry["path"]:
        fail(f"fixture manifest entry has no path: {key}")
    if not isinstance(entry.get("bytes"), int) or entry["bytes"] <= 0:
        fail(f"fixture manifest entry has invalid byte count: {key}")
    if not isinstance(entry.get("sha256"), str) or not sha_pattern.match(entry["sha256"]):
        fail(f"fixture manifest entry has invalid SHA-256: {key}")
    verify_fixture_file(entry, key)

for key in optional_manifest_entries:
    entry = manifest.get(key)
    if entry is None:
        fail(f"fixture manifest missing optional entry: {key}")
    if not isinstance(entry, dict):
        fail(f"fixture manifest optional entry is not an object: {key}")
    if entry.get("exists") is True:
        if not isinstance(entry.get("path"), str) or not entry["path"]:
            fail(f"existing optional fixture entry has no path: {key}")
        if not isinstance(entry.get("bytes"), int) or entry["bytes"] <= 0:
            fail(f"existing optional fixture entry has invalid byte count: {key}")
        if not isinstance(entry.get("sha256"), str) or not sha_pattern.match(entry["sha256"]):
            fail(f"existing optional fixture entry has invalid SHA-256: {key}")
        verify_fixture_file(entry, key)
    elif entry.get("exists") is not False:
        fail(f"optional fixture entry must be explicitly existing or missing: {key}")

for source_key, bundle_key in [
    ("source_kernel", "bundle_kernel"),
    ("source_raw_disk", "bundle_raw_disk"),
    ("source_initrd", "bundle_initrd"),
]:
    source = manifest[source_key]
    bundle = manifest[bundle_key]
    if source.get("exists") != bundle.get("exists"):
        fail(f"source/bundle existence mismatch: {source_key} vs {bundle_key}")
    if source.get("exists"):
        if source.get("bytes") != bundle.get("bytes"):
            fail(f"source/bundle byte count mismatch: {source_key} vs {bundle_key}")
        if source.get("sha256") != bundle.get("sha256"):
            fail(f"source/bundle SHA-256 mismatch: {source_key} vs {bundle_key}")

if launch.get("vm_name") != "live-vz-linux":
    fail("launch spec vm_name is not live-vz-linux")
if (launch.get("guest") or {}).get("os") != "ubuntu":
    fail("launch spec guest OS is not ubuntu")
if (launch.get("guest") or {}).get("arch") != "arm64":
    fail("launch spec guest arch is not arm64")
if (launch.get("boot") or {}).get("mode") != "linux-kernel":
    fail("launch spec boot mode is not linux-kernel")
if not ((launch.get("boot") or {}).get("kernel") or {}).get("exists"):
    fail("launch spec kernel is not marked existing")
if ((launch.get("disk") or {}).get("format")) != "raw":
    fail("launch spec disk format is not raw")
if ((launch.get("disk") or {}).get("read_only")) is not False:
    fail("launch spec disk is not writable")
if not (launch.get("resources") or {}).get("memory"):
    fail("launch spec memory resource is missing")
if not (launch.get("resources") or {}).get("cpu"):
    fail("launch spec CPU resource is missing")
if ((launch.get("resources") or {}).get("balloon_device")) is not True:
    fail("launch spec balloon device is not enabled")
if ((launch.get("devices") or {}).get("network")) != "nat":
    fail("launch spec network is not nat")
if not ((launch.get("devices") or {}).get("serial_log_path")):
    fail("launch spec serial log path is missing")
if not ((launch.get("logs") or {}).get("runner_log_path")):
    fail("launch spec runner log path is missing")
if ((launch.get("readiness") or {}).get("ready")) is not True:
    fail("launch spec readiness is not ready")
if (launch.get("readiness") or {}).get("blockers") != []:
    fail("launch spec readiness blockers are not empty")

if environment_values.get("BRIDGEVM_LIVE_VZ_KERNEL") != manifest["source_kernel"]["path"]:
    fail("environment kernel path does not match source kernel evidence")
if environment_values.get("BRIDGEVM_LIVE_VZ_RAW_DISK") != manifest["source_raw_disk"]["path"]:
    fail("environment raw disk path does not match source raw disk evidence")
if environment_values.get("BRIDGEVM_LIVE_VZ_INITRD", "") != manifest["source_initrd"]["path"]:
    fail("environment initrd path does not match source initrd evidence")
if environment_values.get("BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE") != (launch.get("boot") or {}).get("kernel_command_line"):
    fail("environment kernel command line does not match launch spec")
if environment_values.get("BRIDGEVM_LIVE_VZ_MEMORY_MIB") != str((launch.get("resources") or {}).get("memory")):
    fail("environment memory does not match launch spec resources")
if environment_values.get("BRIDGEVM_LIVE_VZ_CPU_COUNT") != str((launch.get("resources") or {}).get("cpu")):
    fail("environment CPU count does not match launch spec resources")

stop_after_seconds = environment_values.get("BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS")
force_stop_grace_seconds = environment_values.get("BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS")
if not stop_after_seconds:
    fail("environment stop-after seconds is missing")
if not force_stop_grace_seconds:
    fail("environment force-stop grace seconds is missing")
if not re.fullmatch(r"[1-9][0-9]*", stop_after_seconds):
    fail("environment stop-after seconds must be a positive integer")
if not re.fullmatch(r"[1-9][0-9]*", force_stop_grace_seconds):
    fail("environment force-stop grace seconds must be a positive integer")

if handoff.get("backend") != "apple-virtualization-framework":
    fail("handoff backend is not apple-virtualization-framework")
handoff_readiness = handoff.get("readiness") if isinstance(handoff.get("readiness"), dict) else {}
handoff_ready = handoff_readiness.get("ready", handoff.get("ready"))
if handoff_ready is not True:
    fail("handoff is not ready")
if handoff.get("vm_name") != "live-vz-linux":
    fail("handoff vm_name is not live-vz-linux")
if handoff.get("boot_mode") not in (None, "linux-kernel"):
    fail("handoff boot mode is not linux-kernel")
if handoff.get("launch_spec_path") and Path(handoff["launch_spec_path"]).name != "apple-vz-launch.json":
    fail("handoff launch spec path does not point at apple-vz-launch.json")

launch_kernel_path = ((launch.get("boot") or {}).get("kernel") or {}).get("path")
launch_disk_path = (launch.get("disk") or {}).get("path")
launch_runner_log_path = ((launch.get("logs") or {}).get("runner_log_path"))
launch_serial_log_path = ((launch.get("devices") or {}).get("serial_log_path"))
if launch_kernel_path != manifest["bundle_kernel"]["path"]:
    fail("launch kernel path does not match bundled kernel evidence")
if launch_disk_path != manifest["bundle_raw_disk"]["path"]:
    fail("launch disk path does not match bundled raw disk evidence")
if f"Kernel: {launch_kernel_path} " not in launch_output:
    fail("live launch output kernel diagnostic does not match launch spec")
if f"Disk: {launch_disk_path} " not in launch_output:
    fail("live launch output disk diagnostic does not match launch spec")
if handoff.get("runner_log_path") and handoff["runner_log_path"] != launch_runner_log_path:
    fail("handoff runner log path does not match launch spec")
if handoff.get("serial_log_path") and handoff["serial_log_path"] != launch_serial_log_path:
    fail("handoff serial log path does not match launch spec")
runner_log = require_evidence_file(launch_runner_log_path, "runner log referenced by launch spec")

expected = environment_values.get("BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED")
if expected and expected != "<unset>":
    if f"required sentinel found: {expected}" not in summary:
        fail("summary does not prove the configured serial sentinel was found")
    serial_path = (launch.get("devices") or {}).get("serial_log_path")
    if not serial_path:
        fail("serial sentinel configured but launch spec has no serial log path")
    serial_log = require_evidence_file(serial_path, "serial log referenced by launch spec")
    if expected not in serial_log.read_text(encoding="utf-8", errors="replace"):
        fail("serial log does not contain the configured sentinel")

if guest_tools_effects_path.exists():
    guest_tools = load_json(guest_tools_effects_path)
    if guest_tools.get("proven") is not True:
        fail("guest-tools effects evidence is not proven")
    if guest_tools.get("backend") != "bridgevm-tools-linux":
        fail("guest-tools effects backend is not bridgevm-tools-linux")
    command = guest_tools.get("command")
    if not isinstance(command, dict):
        fail("guest-tools effects command record is missing")
    command_request_id = command.get("request_id")
    if not isinstance(command_request_id, str) or not command_request_id:
        fail("guest-tools effects command request_id is missing")
    if command.get("status") != "ok":
        fail("guest-tools effects command status is not ok")
    effects = guest_tools.get("effects")
    if not isinstance(effects, list) or not effects:
        fail("guest-tools effects has no effect records")
    for index, effect in enumerate(effects):
        if not isinstance(effect, dict):
            fail(f"guest-tools effect {index} is not an object")
        if effect.get("request_id") != command_request_id:
            fail(f"guest-tools effect {index} request_id does not match command")
        if effect.get("ok") is not True:
            fail(f"guest-tools effect {index} is not ok")
        if not isinstance(effect.get("kind"), str) or not effect["kind"]:
            fail(f"guest-tools effect {index} kind is missing")
        if not isinstance(effect.get("observation"), str) or not effect["observation"]:
            fail(f"guest-tools effect {index} observation is missing")
        expected_value = effect.get("expected_value")
        observed_value = effect.get("observed_value")
        if isinstance(expected_value, str) and isinstance(observed_value, str):
            if not expected_value:
                fail(f"guest-tools effect {index} expected_value is empty")
            if observed_value != expected_value:
                fail(f"guest-tools effect {index} observed_value does not match expected_value")
            continue
        artifact = effect.get("artifact")
        sha256 = effect.get("sha256")
        if isinstance(artifact, str) or isinstance(sha256, str):
            if not isinstance(artifact, str) or not artifact:
                fail(f"guest-tools effect {index} artifact is empty")
            if not isinstance(sha256, str) or not sha_pattern.match(sha256):
                fail(f"guest-tools effect {index} sha256 is not a SHA-256 hex digest")
            artifact_path = require_evidence_file(artifact, f"guest-tools effect {index} artifact")
            actual_sha256 = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
            if actual_sha256 != sha256:
                fail(f"guest-tools effect {index} sha256 does not match artifact")
            continue
        fail(f"guest-tools effect {index} needs expected_value/observed_value or artifact/sha256 evidence")

def verify_graphical_png_evidence(evidence_path, expected_kind, label):
    evidence = load_json(evidence_path)
    if evidence.get("proven") is not True:
        fail(f"{label} is not proven")
    if evidence.get("kind") != expected_kind:
        fail(f"{label} kind is not {expected_kind}")
    artifact = evidence.get("artifact")
    if not isinstance(artifact, str) or not artifact:
        fail(f"{label} artifact is missing")
    artifact_path = require_relative_evidence_file(artifact, f"{label} artifact")
    if artifact_path.stat().st_size <= 0:
        fail(f"{label} artifact is empty: {artifact_path}")
    for key in ["width", "height"]:
        value = evidence.get(key)
        if not isinstance(value, int) or value <= 0:
            fail(f"{label} {key} must be a positive integer")
    sha256 = evidence.get("sha256")
    if not isinstance(sha256, str) or not sha_pattern.match(sha256):
        fail(f"{label} sha256 is not a SHA-256 hex digest")
    artifact_bytes = artifact_path.read_bytes()
    actual_sha256 = hashlib.sha256(artifact_bytes).hexdigest()
    if sha256 != actual_sha256:
        fail(f"{label} sha256 does not match artifact")
    dimensions = png_dimensions(artifact_bytes)
    if dimensions is None:
        fail(f"{label} artifact is not a PNG image")
    if dimensions != (evidence["width"], evidence["height"]):
        fail(f"{label} width and height do not match artifact pixels")
    observation = evidence.get("observation")
    if not isinstance(observation, str) or not observation:
        fail(f"{label} observation is missing")
    return evidence

if viewer_evidence_path.exists():
    verify_graphical_png_evidence(viewer_evidence_path, "graphical-viewer", "viewer evidence")

if boot_progress_evidence_path.exists():
    boot_progress = verify_graphical_png_evidence(
        boot_progress_evidence_path,
        "graphical-boot-progress",
        "boot progress evidence",
    )
    for key in ["stage", "progress_marker"]:
        value = boot_progress.get(key)
        if not isinstance(value, str) or not value:
            fail(f"boot progress evidence {key} is missing")

if f"BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS={stop_after_seconds}" not in launch_output:
    fail("live launch output stop-after bound does not match environment evidence")
if f"BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS={force_stop_grace_seconds}" not in launch_output:
    fail("live launch output force-stop grace bound does not match environment evidence")

for label in [
    "Store:",
    "Bundle:",
    "Launch spec:",
    "Handoff JSON:",
    "Runner log:",
    "Serial log:",
    "Fixture manifest:",
    "Environment:",
    "Validation output:",
    "Live launch output:",
]:
    if label not in summary:
        fail(f"summary missing line label: {label}")

summary_values = {}
for line in summary.splitlines():
    if ": " not in line:
        continue
    label, value = line.split(": ", 1)
    summary_values[label] = value

expected_summary_paths = {
    "Store": str(manifest.get("store")),
    "Bundle": str(launch.get("bundle_path")),
    "Launch spec": str(launch_path),
    "Handoff JSON": str(handoff_path),
    "Runner log": str(launch_runner_log_path),
    "Serial log": str(launch_serial_log_path),
    "Fixture manifest": str(manifest_path),
    "Environment": str(environment_path),
    "Validation output": str(validate_output_path),
    "Live launch output": str(launch_output_path),
}

for label, expected_path in expected_summary_paths.items():
    actual_path = summary_values.get(label)
    if actual_path != expected_path:
        fail(f"summary {label} path does not match preserved evidence: {actual_path} != {expected_path}")

expected_summary_values = {
    "Stop after seconds": stop_after_seconds,
    "Force stop grace seconds": force_stop_grace_seconds,
}
for label, expected_value in expected_summary_values.items():
    actual_value = summary_values.get(label)
    if actual_value != expected_value:
        fail(f"summary {label} does not match environment evidence: {actual_value} != {expected_value}")
PY

runner_path="$(head -n 1 "$RUNNER_PATH_FILE")"
[[ -n "$runner_path" ]] || fail "AppleVzRunner path file is empty"
runner_artifact="$(head -n 1 "$RUNNER_ARTIFACT_FILE")"
[[ -n "$runner_artifact" ]] || fail "AppleVzRunner artifact file is empty"
[[ "$runner_artifact" != /* && "$runner_artifact" != *".."* ]] || \
  fail "AppleVzRunner artifact must be a relative evidence path: $runner_artifact"
runner_check_path="$EVIDENCE_DIR/$runner_artifact"
[[ -f "$runner_check_path" ]] || fail "AppleVzRunner evidence does not point at a file: $runner_check_path"
[[ ! -L "$runner_check_path" ]] || fail "AppleVzRunner evidence must not be a symlink: $runner_check_path"
[[ -x "$runner_check_path" ]] || fail "AppleVzRunner evidence is not executable: $runner_check_path"
runner_sha="$(head -n 1 "$RUNNER_SHA_FILE")"
assert_sha256_hex "$runner_sha" "AppleVzRunner SHA-256"
actual_runner_sha="$(shasum -a 256 "$runner_check_path" | awk '{print $1}')"
if [[ "$actual_runner_sha" != "$runner_sha" ]]; then
  fail "AppleVzRunner SHA-256 does not match path evidence: $actual_runner_sha != $runner_sha"
fi
environment_runner="$(grep -E '^BRIDGEVM_LIVE_VZ_RUNNER=' "$ENVIRONMENT_FILE" | head -n 1 | cut -d= -f2-)"
if [[ "$environment_runner" != "<auto-build>" && "$environment_runner" != "$runner_path" ]]; then
  fail "environment runner path does not match AppleVzRunner path evidence: $environment_runner != $runner_path"
fi

echo "PASS: Apple VZ live evidence verification ($EVIDENCE_DIR)"
