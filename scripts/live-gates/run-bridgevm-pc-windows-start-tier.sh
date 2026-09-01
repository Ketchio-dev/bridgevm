#!/usr/bin/env bash
# T14: sealed single-run Windows Boot Manager start diagnostic on the independent board.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown BridgeVM PC Windows-start tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" ]] || { echo "T14 needs --out and --input-manifest" >&2; exit 2; }
mkdir -p "$OUT/artifacts"

seal() {
  [[ -f "$1" && ! -L "$1" ]] || { printf absent; return; }
  openssl dgst -sha256 -r "$1" | cut -d' ' -f1 | tr -d '\n'
}
manifest_path() { awk -F '\t' -v key="$1" '$1 == key { print $2 }' "$INPUT_MANIFEST"; }
manifest_hash() { awk -F '\t' -v key="$1" '$1 == key { print $3 }' "$INPUT_MANIFEST"; }
refuse() { echo "FAIL: $1 ($2)" >&2; exit 1; }

COMMIT="$(git -C "$REPO" rev-parse HEAD)"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
MANIFEST_HASH="$(seal "$INPUT_MANIFEST")"
IMAGE_HASH=absent; VARS_HASH=absent; FIRMWARE_HASH=absent; BINARY_HASH=absent
attempted=0; passes=0; work=""

finish() {
  local status=$? outcome=failed passed=false confounders='["diagnostic_incomplete"]'
  trap - EXIT
  if [[ "$status" -eq 0 && "$attempted" -eq 1 && "$passes" -eq 1 ]]; then
    outcome=completed; passed=true
    confounders='["single_run","windows_kernel_entry_not_proven"]'
  fi
  cat > "$OUT/receipt.json" <<EOF
{
  "tier": "t14-bridgevm-pc-windows-start",
  "gate_id": "bridgevm-pc-windows-boot-manager-start-single-diagnostic",
  "tested_commit": "$COMMIT",
  "commit": "$COMMIT",
  "job_id": "$JOB_ID",
  "input_manifest_sha256": "$MANIFEST_HASH",
  "image_sha256": "$IMAGE_HASH",
  "vars_sha256": "$VARS_HASH",
  "gate_asset_hash": "$FIRMWARE_HASH",
  "binary_hash": "$BINARY_HASH",
  "host_os": "$(sw_vers -productVersion)",
  "host_hardware": "$(sysctl -n hw.model)",
  "started_at": "$STARTED_AT",
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "sample_count": $attempted,
  "required_run_count": 1,
  "run_count": $passes,
  "passes": $passes,
  "failures": $((attempted - passes)),
  "evidence_paths": ["diagnostic.log"],
  "known_confounders": $confounders,
  "outcome": "$outcome",
  "pass": $passed
}
EOF
  [[ -z "$work" ]] || rm -rf "$work"
  exit "$status"
}
trap finish EXIT
[[ -f "$INPUT_MANIFEST" && ! -L "$INPUT_MANIFEST" ]] || refuse 'manifest is absent or a symlink' refused-manifest
[[ "$(awk -F '\t' '$1 == "image" || $1 == "vars" { count++ } END { print count + 0 }' "$INPUT_MANIFEST")" -eq 2 ]] \
  || refuse 'manifest must contain exactly image and vars rows' refused-manifest
[[ "$(awk -F '\t' '$1 == "image" { count++ } END { print count + 0 }' "$INPUT_MANIFEST")" -eq 1 \
  && "$(awk -F '\t' '$1 == "vars" { count++ } END { print count + 0 }' "$INPUT_MANIFEST")" -eq 1 \
  && "$(wc -l < "$INPUT_MANIFEST" | tr -d ' ')" -eq 2 ]] \
  || refuse 'manifest has duplicate or unknown rows' refused-manifest
IMAGE="$(manifest_path image)"; VARS="$(manifest_path vars)"
EXPECTED_IMAGE_HASH="$(manifest_hash image)"; EXPECTED_VARS_HASH="$(manifest_hash vars)"
[[ "$IMAGE" == /* && "$VARS" == /* ]] || refuse 'manifest paths must be absolute' refused-manifest
[[ "$EXPECTED_IMAGE_HASH" =~ ^[0-9a-f]{64}$ && "$EXPECTED_VARS_HASH" =~ ^[0-9a-f]{64}$ ]] \
  || refuse 'manifest hashes are malformed' refused-manifest
[[ -f "$IMAGE" && ! -L "$IMAGE" && -f "$VARS" && ! -L "$VARS" ]] \
  || refuse 'sealed input is absent, non-regular, or a symlink' refused-input
[[ "$(stat -f %z "$IMAGE")" -gt 0 && $(( $(stat -f %z "$IMAGE") % 512 )) -eq 0 \
  && "$(stat -f %z "$VARS")" -eq 65536 ]] \
  || refuse 'image or vars size is invalid' refused-input
IMAGE_HASH="$(seal "$IMAGE")"; VARS_HASH="$(seal "$VARS")"
[[ "$IMAGE_HASH" == "$EXPECTED_IMAGE_HASH" && "$VARS_HASH" == "$EXPECTED_VARS_HASH" ]] \
  || refuse 'sealed input hash changed before clone' refused-input

mkdir -p "$HOME/BridgeVM/work"
work="$(mktemp -d "$HOME/BridgeVM/work/bridgevm-pc-windows-$JOB_ID.XXXXXX")"
disk="$work/disk.raw"; vars="$work/vars.fd"
cp -c "$IMAGE" "$disk"; cp -c "$VARS" "$vars"; chmod 600 "$disk" "$vars"
[[ "$(stat -f %i "$disk")" != "$(stat -f %i "$IMAGE")" \
  && "$(stat -f %i "$vars")" != "$(stat -f %i "$VARS")" ]] \
  || refuse 'lane media did not receive independent inodes' clone-alias
[[ "$(seal "$disk")" == "$IMAGE_HASH" && "$(seal "$vars")" == "$VARS_HASH" ]] \
  || refuse 'lane clones differ from sealed inputs' clone-hash-mismatch

EDK2="${BRIDGEVM_PINNED_EDK2_ROOT:-$HOME/BridgeVM-Workspace/deps/tianocore-edk2-b03a21a}"
"$REPO/scripts/build-bridgevm-pc-boot-firmware.sh" "$EDK2" "$OUT/artifacts"
fd="$OUT/artifacts/BridgeVmPcBoot.fd"
FIRMWARE_HASH="$(seal "$fd")"
cargo build -q --release -p bridgevm-hvf --example bridgevm_pc_boot_live
binary="${CARGO_TARGET_DIR:-$REPO/target}/release/examples/bridgevm_pc_boot_live"
codesign --sign - --entitlements "$REPO/apps/macos/HvfRunner.entitlements" --force "$binary"
BINARY_HASH="$(seal "$binary")"

attempted=1
"$binary" --windows-raw-disk "$fd" "$disk" "$vars" > "$OUT/diagnostic.log" 2>&1 \
  || refuse 'Windows Boot Manager diagnostic process failed' process-failed
grep -q '^BridgeVM Virtual ARM PC Windows Boot Manager diagnostic: COMPLETE$' "$OUT/diagnostic.log" \
  || refuse 'diagnostic completion marker is absent' result-missing
grep -Eq '^stage=7 arch=0xfff filesystems=[1-9][0-9]* image=0x[0-9a-f]+\+0x[0-9a-f]+ gop_handles=[1-9][0-9]* framebuffer=0x[0-9a-f]+\+0x[0-9a-f]+$' "$OUT/diagnostic.log" \
  || refuse 'Windows Boot Manager StartImage handoff was not observed' handoff-missing
grep -q 'boot_media_mode=raw-cow ram_mib=6144 ' "$OUT/diagnostic.log" \
  || refuse 'raw COW or Windows RAM policy was not active' policy-mismatch
grep -q '^windows_boot_proven=false$' "$OUT/diagnostic.log" \
  || refuse 'diagnostic honesty marker is absent' honesty-missing
[[ "$(seal "$IMAGE")" == "$IMAGE_HASH" && "$(seal "$VARS")" == "$VARS_HASH" \
  && "$(seal "$disk")" == "$IMAGE_HASH" ]] \
  || refuse 'source or COW disk changed during diagnostic' source-media-changed
passes=1
