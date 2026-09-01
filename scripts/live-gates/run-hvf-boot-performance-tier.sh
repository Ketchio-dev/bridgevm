#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
source "$REPO/scripts/live-gates/hvf-boot-performance-manifest.sh"
WRITER="$REPO/scripts/live-gates/write-hvf-boot-performance-receipt.py"
OUT=""; INPUT_MANIFEST=""; SEALED_BINARY=""; JOB_ID="local-perf"; VALIDATE_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    *) echo "unknown boot-performance option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" ]] || { echo "boot-performance tier needs --out" >&2; exit 2; }
mkdir -p "$OUT"

power_source() { command -v pmset >/dev/null && pmset -g batt | sed -n "s/^Now drawing from '\(.*\)'/\1/p" || true; }
export PERF_JOB_ID="$JOB_ID" PERF_HARNESS_COMMIT="$(git -C "$REPO" rev-parse HEAD)"
export PERF_BINARY_SOURCE_COMMIT="unknown" PERF_BINARY_PROFILE="unknown" PERF_BINARY_FEATURES="unknown"
export PERF_RUST_TOOLCHAIN="unknown" PERF_BINARY_HASH="absent" PERF_MANIFEST_HASH="absent"
export PERF_IMAGE_HASH="absent" PERF_VARS_HASH="absent" PERF_FIRMWARE_HASH="absent" PERF_CONFIG_HASH="absent"
export PERF_CAMPAIGN_ID="unknown" PERF_CAMPAIGN_MODE="unknown" PERF_CAMPAIGN_ROLE="unknown"
export PERF_CAMPAIGN_ORDINAL=0 PERF_CAMPAIGN_EXPECTED_RUNS=0
export PERF_HOST_MODEL="$(sysctl -n hw.model 2>/dev/null || uname -m)" PERF_MACOS_VERSION="$(sw_vers -productVersion 2>/dev/null || uname -sr)"
export PERF_POWER_SOURCE_START="$(power_source)" PERF_POWER_SOURCE_END="unknown"
[[ -n "$PERF_POWER_SOURCE_START" ]] || PERF_POWER_SOURCE_START="unknown"
export PERF_STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
INVALID_REASON="failed-before-receipt"; RECEIPT_WRITTEN=0

write_receipt() {
  local outcome="$1" passed="$2" desktop="$3" valid="$4" reason="$5"
  RECEIPT_WRITTEN=1; PERF_POWER_SOURCE_END="$(power_source)"
  [[ -n "$PERF_POWER_SOURCE_END" ]] || PERF_POWER_SOURCE_END="unknown"
  if [[ "$PERF_POWER_SOURCE_START" == unknown || "$PERF_POWER_SOURCE_END" == unknown ]]; then
    outcome=failed; passed=false; valid=false; reason="power-source-unknown"
  elif [[ "$PERF_POWER_SOURCE_START" != "$PERF_POWER_SOURCE_END" ]]; then
    outcome=failed; passed=false; valid=false; reason="power-source-changed"
  fi
  export PERF_POWER_SOURCE_END PERF_RECEIPT_OUTCOME="$outcome" PERF_RECEIPT_PASS="$passed"
  export PERF_RECEIPT_DESKTOP="$desktop" PERF_RECEIPT_VALID="$valid" PERF_RECEIPT_REASON="$reason"
  python3 "$WRITER" "$OUT/receipt.json"
}
on_exit() {
  local status="$?"
  [[ "$RECEIPT_WRITTEN" == 1 ]] || write_receipt failed false "" false "$INVALID_REASON"
  return "$status"
}
trap on_exit EXIT

INVALID_REASON="sealed-input-mismatch"
[[ -f "$INPUT_MANIFEST" ]] && PERF_MANIFEST_HASH="$(perf_seal "$INPUT_MANIFEST")"
export PERF_MANIFEST_HASH
perf_manifest_validate "$REPO" "$INPUT_MANIFEST" "$SEALED_BINARY" || exit 1
export PERF_BINARY_SOURCE_COMMIT PERF_BINARY_PROFILE PERF_BINARY_FEATURES PERF_RUST_TOOLCHAIN PERF_BINARY_HASH
export PERF_CAMPAIGN_ID PERF_CAMPAIGN_MODE PERF_CAMPAIGN_ROLE PERF_CAMPAIGN_ORDINAL PERF_CAMPAIGN_EXPECTED_RUNS
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  perf_manifest_verify_source_hashes "$INPUT_MANIFEST" || exit 1
  RECEIPT_WRITTEN=1; echo "HVF boot performance manifest: PASS"; exit 0
fi
INVALID_REASON="unsigned-or-invalid-binary"
codesign --verify --strict "$SEALED_BINARY" >/dev/null 2>&1 || exit 1

TARGET="$(perf_manifest_value image "$INPUT_MANIFEST")"; VARS="$(perf_manifest_value vars "$INPUT_MANIFEST")"
FIRMWARE="$REPO/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
PERF_FIRMWARE_HASH="$(perf_seal "$FIRMWARE")"; export PERF_FIRMWARE_HASH
mkdir -p "$OUT/media" "$OUT/boot"
INVALID_REASON="apfs-clone-failed"
cp -c "$TARGET" "$OUT/media/target.raw" && cp -c "$VARS" "$OUT/media/vars.fd" || exit 1
chmod u=rw,go= "$OUT/media/target.raw" "$OUT/media/vars.fd"
INVALID_REASON="cloned-input-mismatch"
PERF_IMAGE_HASH="$(perf_seal "$OUT/media/target.raw")"; PERF_VARS_HASH="$(perf_seal "$OUT/media/vars.fd")"
export PERF_IMAGE_HASH PERF_VARS_HASH
[[ "$PERF_IMAGE_HASH" == "$(perf_manifest_hash image "$INPUT_MANIFEST")" \
  && "$PERF_VARS_HASH" == "$(perf_manifest_hash vars "$INPUT_MANIFEST")" ]] || exit 1
PERF_CONFIG_HASH="$(printf '%s' "release;skip-build;daily;smp=4;ram=6144;virtio-net;xhci;agent-ready;shutdown;watchdog=120000;firmware=$PERF_FIRMWARE_HASH;warm-cache=clone-integrity-scan" | shasum -a 256 | cut -d' ' -f1)"
export PERF_CONFIG_HASH
printf 'harness_commit=%s\nbinary_source_commit=%s\nbinary_sha256=%s\nimage_sha256=%s\nvars_sha256=%s\nfirmware_sha256=%s\nconfig_sha256=%s\npower_source_start=%s\n' \
  "$PERF_HARNESS_COMMIT" "$PERF_BINARY_SOURCE_COMMIT" "$PERF_BINARY_HASH" "$PERF_IMAGE_HASH" "$PERF_VARS_HASH" "$PERF_FIRMWARE_HASH" "$PERF_CONFIG_HASH" "$PERF_POWER_SOURCE_START" > "$OUT/measurement-identity.txt"

INVALID_REASON="boot-wrapper-failed"; status=0
BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$OUT/media/target.raw" --vars "$OUT/media/vars.fd" --firmware-code "$FIRMWARE" \
  --evidence-dir "$OUT/boot" --release --skip-build --daily --smp-cpus 4 --ram-mib 6144 \
  --watchdog-ms 120000 --boot-timer --boot-timer-desktop-agent --virtio-net --enable-xhci \
  --shutdown-after-agent-ready > "$OUT/boot-wrapper.stdout" 2> "$OUT/boot-wrapper.stderr" || status=$?
report_status=0
"$REPO/scripts/report-hvf-boot-timer-metrics.sh" "$OUT/boot" > "$OUT/boot/boot-timer-report.tsv" || report_status=$?
row="$(awk -F '\t' '$1 == "run" { print; exit }' "$OUT/boot/boot-timer-report.tsv")"
desktop="$(awk -F '\t' '$1 == "run" { print $5; exit }' <<< "$row")"
valid="$(awk -F '\t' '$1 == "run" { print $13; exit }' <<< "$row")"
reason="$(awk -F '\t' '$1 == "run" { print $14; exit }' <<< "$row")"
if [[ "$status" == 0 && "$report_status" == 0 && "$valid" == true && "$desktop" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  INVALID_REASON=""; write_receipt completed true "$desktop" true ""; exit 0
fi
[[ -n "$reason" ]] || reason="wrapper=$status,report=$report_status"
INVALID_REASON="$reason"; write_receipt failed false "${desktop:-}" "${valid:-false}" "$reason"; exit 1
