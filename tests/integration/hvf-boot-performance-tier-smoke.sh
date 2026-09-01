#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
seal() { shasum -a 256 "$1" | cut -d' ' -f1; }
printf image > "$WORK/image"
printf vars > "$WORK/vars"
printf renderer > "$WORK/renderer"
printf '#!/bin/sh\nexit 0\n' > "$WORK/probe"
chmod +x "$WORK/probe"
base="$WORK/base.tsv"; manifest="$WORK/manifest.tsv"
printf 'image\t%s\t%s\n' "$WORK/image" "$(seal "$WORK/image")" > "$base"
printf 'vars\t%s\t%s\n' "$WORK/vars" "$(seal "$WORK/vars")" >> "$base"
printf 'renderer\t%s\t%s\n' "$WORK/renderer" "$(seal "$WORK/renderer")" >> "$base"
printf 'binary\t%s\t%s\nbinary_source_commit\t%s\nbinary_profile\trelease\nbinary_features\tvenus\nrust_toolchain\t1.97.0\n' \
  "$WORK/probe" "$(seal "$WORK/probe")" "$(git -C "$REPO" rev-parse HEAD)" >> "$base"
cp "$base" "$manifest"
printf 'campaign_id\t%032d\ncampaign_mode\tAA\ncampaign_role\tbaseline\ncampaign_ordinal\t1\ncampaign_expected_runs\t6\n' 0 >> "$manifest"
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
printf image > "$WORK/image"; chmod 400 "$base"
"$REPO/scripts/submit-hvf-boot-performance-campaign.sh" --mode AA --pairs 4 --campaign-id "$(printf %032d 1)" --baseline-manifest "$base" >/dev/null
test "$(find "$BRIDGEVM_LIVE_ROOT/queued" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')" = 8
test "$(find "$BRIDGEVM_LIVE_ROOT/queued" -name input-manifest.tsv -exec awk -F '\t' '$1 == "campaign_ordinal" { ordinal=$2 } $1 == "campaign_role" { role=$2 } END { if (NR != 13) exit 1; print ordinal "=" role }' {} \; | sort | paste -sd, -)" = "1=baseline,2=candidate,3=candidate,4=baseline,5=baseline,6=candidate,7=candidate,8=baseline"
python3 "$REPO/scripts/report-hvf-boot-performance-ab.py" --self-test >/dev/null
echo "HVF boot performance tier smoke: PASS"
