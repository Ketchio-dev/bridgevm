#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
seal() { shasum -a 256 "$1" | cut -d' ' -f1; }

printf image > "$WORK/image"
printf vars > "$WORK/vars"
printf '#!/bin/sh\nexit 0\n' > "$WORK/probe"
chmod +x "$WORK/probe"
manifest="$WORK/manifest.tsv"
printf 'image\t%s\t%s\n' "$WORK/image" "$(seal "$WORK/image")" > "$manifest"
printf 'vars\t%s\t%s\n' "$WORK/vars" "$(seal "$WORK/vars")" >> "$manifest"
printf 'binary\t%s\t%s\n' "$WORK/probe" "$(seal "$WORK/probe")" >> "$manifest"

"$REPO/scripts/live-gates/run-hvf-boot-performance-tier.sh" \
  --out "$WORK/validate" --input-manifest "$manifest" \
  --sealed-binary "$WORK/probe" --validate-only >/dev/null
printf changed >> "$WORK/image"
if "$REPO/scripts/live-gates/run-hvf-boot-performance-tier.sh" \
    --out "$WORK/reject" --input-manifest "$manifest" \
    --sealed-binary "$WORK/probe" --validate-only >/dev/null 2>&1; then
  echo "HVF boot performance tier smoke: FAIL (changed image accepted)" >&2
  exit 1
fi

export BRIDGEVM_LIVE_ROOT="$WORK/queue"
printf image > "$WORK/image"
job="$($REPO/scripts/live-gates/bridgevm-live submit t15-hvf-boot-performance \
  --input-manifest "$manifest")"
test -x "$BRIDGEVM_LIVE_ROOT/queued/$job/hvf_gic_boot_probe"
grep -q '^sealed_binary_sha256=[0-9a-f]\{64\}$' "$BRIDGEVM_LIVE_ROOT/queued/$job/job.env"
python3 "$REPO/scripts/report-hvf-boot-performance-ab.py" --self-test >/dev/null
echo "HVF boot performance tier smoke: PASS"
