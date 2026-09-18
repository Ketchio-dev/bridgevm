#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
CONTRACT="$REPO/scripts/hvf_smp_confirmation.py"
OUT="" INPUT_MANIFEST="" SEALED_BINARY="" JOB_ID="local-smp-confirmation"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown SMP confirmation option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -f "$INPUT_MANIFEST" && -f "$SEALED_BINARY" ]] || {
  echo "SMP confirmation tier needs --out, --input-manifest and --sealed-binary" >&2
  exit 2
}
mkdir -p "$OUT"

value() { awk -F '\t' -v key="$1" '$1==key {print $2; exit}' "$INPUT_MANIFEST"; }
hash_value() { awk -F '\t' -v key="$1" '$1==key {print $3; exit}' "$INPUT_MANIFEST"; }
power_source() { pmset -g batt | sed -n "s/^Now drawing from '\(.*\)'/\1/p"; }
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }

python3 "$CONTRACT" validate-manifest "$INPUT_MANIFEST" >/dev/null
SOURCE_COMMIT="$(value binary_source_commit)"
git -C "$REPO" cat-file -e "$SOURCE_COMMIT^{commit}" 2>/dev/null || {
  echo "sealed source commit is unavailable" >&2; exit 1;
}
[[ "$(seal "$SEALED_BINARY")" == "$(hash_value binary)" ]] || {
  echo "queue-sealed binary hash mismatch" >&2; exit 1;
}
codesign --verify --strict "$SEALED_BINARY" >/dev/null 2>&1 || {
  echo "queue-sealed binary signature is invalid" >&2; exit 1;
}
ENTITLEMENTS="$(codesign -d --entitlements :- "$SEALED_BINARY" 2>&1)"
grep -q 'com.apple.security.hypervisor' <<< "$ENTITLEMENTS" || {
  echo "queue-sealed binary lacks the hypervisor entitlement" >&2; exit 1;
}
if grep -q 'com.apple.security.get-task-allow' <<< "$ENTITLEMENTS"; then
  echo "queue-sealed release binary carries get-task-allow" >&2; exit 1
fi
RENDERER="$(value renderer)"
otool -L "$SEALED_BINARY" | grep -Fq "$RENDERER" || {
  echo "queue-sealed binary does not link the sealed renderer" >&2; exit 1;
}

STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
POWER_START="$(power_source)"; [[ -n "$POWER_START" ]] || POWER_START=unknown
COMMIT="$(git -C "$REPO" rev-parse HEAD)"
FIRMWARE="$REPO/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
FIRMWARE_SHA256="$(seal "$FIRMWARE")"
RECEIPT_WRITTEN=0 INVALID_REASON="confirmation-runner-failed"

write_receipt() {
  local reason="$1" finished power_end
  finished="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  power_end="$(power_source)"; [[ -n "$power_end" ]] || power_end=unknown
  python3 "$CONTRACT" write-receipt \
    --manifest "$INPUT_MANIFEST" --matrix-root "$OUT/matrix" \
    --output "$OUT/receipt.json" --job-id "$JOB_ID" --commit "$COMMIT" \
    --started-at "$STARTED" --finished-at "$finished" \
    --host-model "$(sysctl -n hw.model)" --macos-version "$(sw_vers -productVersion)" \
    --power-start "$POWER_START" --power-end "$power_end" \
    --firmware-sha256 "$FIRMWARE_SHA256" --invalid-reason "$reason"
  RECEIPT_WRITTEN=1
}
on_exit() {
  local status="$?"
  if [[ "$RECEIPT_WRITTEN" == 0 ]]; then
    write_receipt "$INVALID_REASON" >/dev/null 2>&1 || true
  fi
  return "$status"
}
trap on_exit EXIT

TARGET="$(value image)" VARS="$(value vars)"
for pair in $(seq 1 12); do
  printf -v pair_name 'pair-%02d' "$pair"
  order="4,6"; (( pair % 2 == 1 )) || order="6,4"
  INVALID_REASON="${pair_name}-execution-failed"
  BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" "$REPO/scripts/run-hvf-boot-timer-matrix.sh" \
    --target "$TARGET" --vars "$VARS" --evidence-dir "$OUT/matrix/$pair_name" \
    --runs 1 --smp-cpus "$order" --release --skip-build \
    --expected-target-sha256 "$(hash_value image)" --expected-vars-sha256 "$(hash_value vars)" \
    -- --daily --ram-mib 6144 --watchdog-ms 120000 --virtio-net --enable-xhci \
    --hda-coreaudio --performance-risk balanced --shutdown-after-agent-ready
done

INVALID_REASON="receipt-validation-failed"
write_receipt ""
python3 "$CONTRACT" verify-receipt "$OUT/receipt.json" --expected-commit "$COMMIT" >/dev/null
INVALID_REASON=""
echo "HVF SMP confirmation diagnostic: PASS"
