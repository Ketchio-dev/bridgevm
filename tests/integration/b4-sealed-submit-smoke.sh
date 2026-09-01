#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
export BRIDGEVM_LIVE_ROOT="$WORK/queue"; CLI="$ROOT/scripts/live-gates/bridgevm-live"
asset="$ROOT/scripts/win-assets/bvgpu-apply-host-resolution.ps1"
grep -q 'Get-RequestedMode' "$asset"; grep -q 'change_attempt=' "$asset"; ! grep -q '^\$dm = \$before' "$asset"
printf probe > "$WORK/media"; mkdir "$WORK/driver"; printf driver > "$WORK/driver/file"
file_hash="$(shasum -a 256 "$WORK/media" | cut -d' ' -f1)"
tree_hash="$(cd "$WORK/driver" && find . -type f -exec shasum -a 256 {} + | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1)"
manifest="$WORK/inputs.tsv"
printf 'image\t%s\t%s\nvars\t%s\t%s\nviogpu_dir\t%s\t%s\n' \
  "$WORK/media" "$file_hash" "$WORK/media" "$file_hash" "$WORK/driver" "$tree_hash" > "$manifest"
job=b4.sealed-submit-1; test "$("$CLI" submit t8-pointer-reliability --input-manifest "$manifest" --job-id "$job")" = "$job"; dir="$BRIDGEVM_LIVE_ROOT/queued/$job"
test -f "$dir/input-manifest.tsv"; grep -q '^input_manifest_sha256=[0-9a-f]\{64\}$' "$dir/job.env"; ledger="$BRIDGEVM_LIVE_ROOT/job-ledger/$job/entry.env"; grep -q '^input_manifest_sha256=[0-9a-f]\{64\}$' "$ledger"; grep -Fxq 'sealed_binary_sha256=' "$ledger"
test ! -e "$dir/hvf_gic_boot_probe"
printf changed >> "$manifest"; cmp -s "$dir/input-manifest.tsv" "$manifest" && exit 1
echo 'PASS: B4 submission seals its exact private inputs'
