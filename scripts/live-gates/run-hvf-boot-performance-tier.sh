#!/usr/bin/env bash
# One sealed release boot sample for interleaved A/A and A/B campaigns.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
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

seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
manifest_field() { awk -F '\t' -v key="$1" '$1 == key { print $2; exit }' "$INPUT_MANIFEST"; }
manifest_hash() { awk -F '\t' -v key="$1" '$1 == key { print $3; exit }' "$INPUT_MANIFEST"; }
IMAGE_HASH="absent"; VARS_HASH="absent"; BINARY_HASH="absent"; CONFIG_HASH="absent"
POWER_SOURCE="unknown"; INVALID_REASON="failed-before-receipt"; STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RECEIPT_WRITTEN=0

write_receipt() {
  local outcome="$1" pass="$2" desktop="$3" valid="$4" reason="$5"
  RECEIPT_WRITTEN=1
  python3 - "$OUT/receipt.json" "$outcome" "$pass" "$desktop" "$valid" "$reason" \
    "$JOB_ID" "$(git -C "$REPO" rev-parse HEAD)" "$IMAGE_HASH" "$VARS_HASH" \
    "$BINARY_HASH" "$CONFIG_HASH" "$POWER_SOURCE" "$STARTED_AT" <<'PY'
import json, sys
path, outcome, passed, desktop, valid, reason, job, commit, image, vars_, binary, config, power, started = sys.argv[1:]
ok = passed == "true"
receipt = {
    "tier": "t15-hvf-boot-performance", "gate_id": "hvf-boot-performance-diagnostic",
    "job_id": job, "commit": commit, "tested_commit": commit,
    "image_sha256": image, "vars_sha256": vars_, "binary_hash": binary,
    "config_sha256": config, "host_model": __import__("subprocess").check_output(["sysctl", "-n", "hw.model"], text=True).strip(),
    "macos_version": __import__("platform").mac_ver()[0], "power_source": power,
    "smp_cpus": 4, "ram_mib": 6144, "desktop_elapsed_ms": float(desktop) if desktop else None,
    "valid": valid == "true", "invalid_reason": reason, "sample_count": 1,
    "run_count": 1, "required_run_count": 1, "passes": 1 if ok else 0,
    "failures": 0 if ok else 1, "started_at": started,
    "finished_at": __import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat().replace("+00:00", "Z"),
    "outcome": outcome, "pass": ok, "evidence_paths": ["boot/boot-timer-report.tsv"],
    "known_confounders": [],
}
with open(path, "w") as handle:
    json.dump(receipt, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY
}
on_exit() {
  local status="$?"
  [[ "$RECEIPT_WRITTEN" == 1 ]] || write_receipt failed false "" false "$INVALID_REASON"
  return "$status"
}
trap on_exit EXIT

verify_manifest() {
  [[ -f "$INPUT_MANIFEST" && "$SEALED_BINARY" == /* && -f "$SEALED_BINARY" && ! -L "$SEALED_BINARY" ]] || return 1
  awk -F '\t' 'NF != 3 { exit 1 } !($1 == "image" || $1 == "vars" || $1 == "binary") { exit 1 }
    $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/ { exit 1 } { seen[$1]++ }
    END { exit !(NR == 3 && seen["image"] == 1 && seen["vars"] == 1 && seen["binary"] == 1) }' "$INPUT_MANIFEST" || return 1
  local image vars
  image="$(manifest_field image)"; vars="$(manifest_field vars)"
  [[ -f "$image" && ! -L "$image" && -f "$vars" && ! -L "$vars" ]] || return 1
  IMAGE_HASH="$(seal "$image")"; VARS_HASH="$(seal "$vars")"; BINARY_HASH="$(seal "$SEALED_BINARY")"
  [[ "$IMAGE_HASH" == "$(manifest_hash image)" && "$VARS_HASH" == "$(manifest_hash vars)" \
    && "$BINARY_HASH" == "$(manifest_hash binary)" ]]
}

INVALID_REASON="sealed-input-mismatch"
verify_manifest || exit 1
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  RECEIPT_WRITTEN=1
  echo "HVF boot performance manifest: PASS"
  exit 0
fi
INVALID_REASON="unsigned-or-invalid-binary"
codesign --verify --strict "$SEALED_BINARY" >/dev/null 2>&1 || exit 1

TARGET="$(manifest_field image)"; VARS="$(manifest_field vars)"
CONFIG_HASH="$(printf '%s' 'release;smp=4;ram=6144;daily;virtio-net;xhci;agent-ready;shutdown;watchdog=120000' | shasum -a 256 | cut -d' ' -f1)"
POWER_SOURCE="$(pmset -g batt | sed -n "s/^Now drawing from '\(.*\)'/\1/p")"
[[ -n "$POWER_SOURCE" ]] || POWER_SOURCE="unknown"
mkdir -p "$OUT/media" "$OUT/boot"
INVALID_REASON="apfs-clone-failed"
cp -c "$TARGET" "$OUT/media/target.raw" && cp -c "$VARS" "$OUT/media/vars.fd" || exit 1
{
  printf 'source_commit=%s\n' "$(git -C "$REPO" rev-parse HEAD)"
  printf 'binary_sha256=%s\nimage_sha256=%s\nvars_sha256=%s\n' "$BINARY_HASH" "$IMAGE_HASH" "$VARS_HASH"
  printf 'config_sha256=%s\npower_source=%s\n' "$CONFIG_HASH" "$POWER_SOURCE"
} > "$OUT/measurement-identity.txt"

INVALID_REASON="boot-wrapper-failed"
status=0
BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$OUT/media/target.raw" --vars "$OUT/media/vars.fd" --evidence-dir "$OUT/boot" \
  --release --skip-build --daily --smp-cpus 4 --ram-mib 6144 --watchdog-ms 120000 \
  --boot-timer --boot-timer-desktop-agent --virtio-net --enable-xhci \
  --shutdown-after-agent-ready > "$OUT/boot-wrapper.stdout" 2> "$OUT/boot-wrapper.stderr" || status=$?
report_status=0
"$REPO/scripts/report-hvf-boot-timer-metrics.sh" "$OUT/boot" \
  > "$OUT/boot/boot-timer-report.tsv" || report_status=$?
row="$(awk -F '\t' '$1 == "run" { print; exit }' "$OUT/boot/boot-timer-report.tsv")"
desktop="$(awk -F '\t' '$1 == "run" { print $5; exit }' <<< "$row")"
valid="$(awk -F '\t' '$1 == "run" { print $13; exit }' <<< "$row")"
reason="$(awk -F '\t' '$1 == "run" { print $14; exit }' <<< "$row")"
if [[ "$status" == 0 && "$report_status" == 0 && "$valid" == true && "$desktop" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  INVALID_REASON=""
  write_receipt completed true "$desktop" true ""
  exit 0
fi
[[ -n "$reason" ]] || reason="wrapper=$status,report=$report_status"
INVALID_REASON="$reason"
write_receipt failed false "${desktop:-}" "${valid:-false}" "$reason"
exit 1
