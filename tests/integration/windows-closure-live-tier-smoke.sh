#!/usr/bin/env bash
# Deterministic policy/mutation checks for the exact-input Windows closure tier.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MANIFEST_HELPER="$ROOT/scripts/live-gates/windows-closure-manifest.sh"
TIER="$ROOT/scripts/live-gates/run-windows-closure-tier.sh"
INTERACT="$ROOT/scripts/windows-1.0-closure-interact.sh"
PROOF="$ROOT/scripts/win-assets/bv-windows-closure-proof.ps1"
RECEIPT="$ROOT/scripts/live-gates/write-windows-closure-receipt.py"
CLI="$ROOT/scripts/live-gates/bridgevm-live"
WORKER="$ROOT/scripts/live-gates/bridgevm-live-worker.sh"
DISPATCH="$ROOT/scripts/live-gates/run-tier.sh"
MISSING="$ROOT/scripts/live-gates/write-missing-receipt.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

for executable in "$MANIFEST_HELPER" "$TIER" "$INTERACT" "$RECEIPT" "$MISSING"; do
  [[ -x "$executable" ]] || { echo "FAIL: not executable: $executable" >&2; exit 1; }
done
[[ -f "$PROOF" ]] || { echo "FAIL: missing guest proof" >&2; exit 1; }
python3 "$RECEIPT" --self-test | grep -q PASS

# Manifest parser accepts only one exact entry for every required key and uses
# the copied sealed binary, never a caller-owned binary path, for verification.
rg -q 'image vars injector injector_assets agent viogpu_dir virglrenderer moltenvk binary' "$MANIFEST_HELPER"
rg -q 'actual="\$\(seal "\$SEALED_BINARY"\)"' "$MANIFEST_HELPER"
rg -q 't7-windows-closure' "$CLI" "$WORKER" "$DISPATCH" "$MISSING" && rg -q 'verify-windows-closure-binary.sh' "$TIER"

# Safety invariants that must fail if somebody regresses to booting originals
# or sharing a writable vars file between injection and proof.
rg -q 'cp -c "\$IMAGE" "\$STAGE/disk.raw"' "$TIER"
rg -q 'cp "\$VARS" "\$STAGE/vars.fd"' "$TIER"
rg -q 'cp -c "\$RETAINED/disk.raw" "\$PROOF_WORK/disk.raw"' "$TIER"
rg -q 'cp "\$RETAINED/vars.fd" "\$PROOF_WORK/vars.fd"' "$TIER"
rg -q 'SOURCE_IMAGE_HASH.*SOURCE_VARS_HASH' "$TIER"
rg -q 'prepared pair changed during proof' "$TIER"
rg -q 'injector_boot_observed' "$TIER"
rg -q 'chmod 400.*disk.raw.*vars.fd' "$TIER"

# F1-F4 are checked from live outputs; a capture must be explicitly identified
# as the virtio-gpu checkpoint and the shipped verbs must traverse the channel.
rg -q 'BVF1MODE.*has_1600x900' "$INTERACT"
rg -q 'RESIZE 1600x900.*SET_SCANOUT.*rect_w.*1600.*rect_h.*900' <(tr '\n' ' ' < "$INTERACT")
for verb in WINLIST WINBOUNDS WINFOCUS WINCLOSE; do rg -q "$verb" "$INTERACT"; done
rg -q 'virtio-gpu-checkpoint-' "$INTERACT"
rg -q 'tesseract' "$INTERACT"
rg -q "ValidateSet\('F1', 'Display', 'Window', 'Notepad'\)" "$PROOF"

# Submission smoke: T7 requires a manifest, copies it, and seals its binary.
export BRIDGEVM_LIVE_ROOT="$TMP/queue"
probe="$TMP/probe"; printf '#!/bin/sh\nexit 0\n' > "$probe"; chmod +x "$probe"
manifest="$TMP/manifest.tsv"
for key in image vars injector injector_assets agent virglrenderer moltenvk; do
  printf '%s' "$key" > "$TMP/$key"
  printf '%s\t%s\t%s\n' "$key" "$TMP/$key" "$(shasum -a 256 "$TMP/$key" | cut -d' ' -f1)" >> "$manifest"
done
mkdir "$TMP/viogpu"; printf driver > "$TMP/viogpu/file"
tree_hash="$(cd "$TMP/viogpu" && find . -type f -exec shasum -a 256 {} + | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1)"
printf 'viogpu_dir\t%s\t%s\n' "$TMP/viogpu" "$tree_hash" >> "$manifest"
printf 'binary\t%s\t%s\n' "$probe" "$(shasum -a 256 "$probe" | cut -d' ' -f1)" >> "$manifest"
! "$CLI" submit t7-windows-closure >/dev/null 2>&1
job="$($CLI submit t7-windows-closure --input-manifest "$manifest")"
[[ -f "$BRIDGEVM_LIVE_ROOT/queued/$job/input-manifest.tsv" ]]
grep -q '^sealed_binary_sha256=[0-9a-f]\{64\}$' "$BRIDGEVM_LIVE_ROOT/queued/$job/job.env"

echo 'PASS: Windows closure live tier policy smoke'
