#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-windows-start-tier.sh"
SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
TIER="$REPO/scripts/live-gates/run-tier.sh"
CLI="$REPO/scripts/live-gates/bridgevm-live"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"

grep -q 'boot-manager-start-single-diagnostic' "$RUN"
grep -q 'cp -c "$IMAGE" "$disk"' "$RUN"
grep -q 'cp -c "$VARS" "$vars"' "$RUN"
grep -q -- '--windows-raw-disk' "$RUN"
grep -q 'windows_boot_proven=false' "$RUN"
grep -q 't14-bridgevm-pc-windows-start)' "$SPECIAL"
grep -q 't14-bridgevm-pc-windows-start' "$TIER"
grep -q 't14-bridgevm-pc-windows-start' "$WORKER"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
image="$work/image"; vars="$work/vars"; manifest="$work/manifest.tsv"
dd if=/dev/zero of="$image" bs=512 count=2 status=none
dd if=/dev/zero of="$vars" bs=65536 count=1 status=none
printf 'image\t%s\t%s\n' "$image" "$(shasum -a 256 "$image" | cut -d' ' -f1)" > "$manifest"
printf 'vars\t%s\t%s\n' "$vars" "$(shasum -a 256 "$vars" | cut -d' ' -f1)" >> "$manifest"
export BRIDGEVM_LIVE_ROOT="$work/queue"
job="$($CLI submit t14-bridgevm-pc-windows-start --input-manifest "$manifest")"
queued="$BRIDGEVM_LIVE_ROOT/queued/$job"
test -f "$queued/input-manifest.tsv"
grep -q '^input_manifest_sha256=[0-9a-f]\{64\}$' "$queued/job.env"
test ! -e "$queued/hvf_gic_boot_probe"

bad="$work/bad.tsv"; bad_out="$work/bad-out"
printf 'image\t%s\t%s\n' "$image" "$(shasum -a 256 "$image" | cut -d' ' -f1)" > "$bad"
! "$RUN" --out "$bad_out" --input-manifest "$bad" --job-id smoke >/dev/null 2>&1
grep -q '"pass": false' "$bad_out/receipt.json"
grep -q '"known_confounders": \["diagnostic_incomplete"\]' "$bad_out/receipt.json"
echo "bridgevm pc Windows-start live tier smoke: PASS"
