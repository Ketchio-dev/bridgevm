#!/usr/bin/env bash
# T12: fixed N=20 standard UEFI NVMe Block I/O reads on the independent board.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; SEALED_BINARY=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
    *) echo "unknown BridgeVM PC NVMe Block I/O tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$SEALED_BINARY" ]] || { echo "T12 requires submitted sealed inputs" >&2; exit 2; }
mkdir -p "$OUT/lanes" "$OUT/artifacts"
readonly REQUIRED_LANES=20

source "$REPO/scripts/live-gates/t12-receipt.sh"
trap finish EXIT

printf 'lane\tresult\n' > "$OUT/summary.tsv"
inputs="$(python3 "$REPO/scripts/live-gates/t12_inputs.py" run "$OUT" "$commit" "$JOB_ID" "$INPUT_MANIFEST" "$SEALED_BINARY")"
read -r input_manifest_hash binary_hash gate_asset_hash image_hash initial_vars_hash rust_toolchain <<< "$inputs"
fd="$OUT/t12-firmware.fd"; binary="$SEALED_BINARY"
for lane in $(seq 1 "$REQUIRED_LANES"); do
  lane_name="$(printf 'lane-%02d' "$lane")"; lane_dir="$OUT/lanes/$lane_name"
  mkdir -p "$lane_dir"; attempted=$((attempted + 1))
  if "$REPO/tests/integration/bridgevm-pc-variable-process-persistence.sh" \
      "$binary" "$fd" "$lane_dir" > "$lane_dir/run.log" 2>&1 &&
     [[ "$(grep -Fc "vars_loaded_sha256=$initial_vars_hash" "$lane_dir/run.log" || true)" -eq 1 ]] &&
     [[ "$(grep -Ec 'nvme_bar_reads=2 nvme_bar0=0x[0-9a-f]+ nvme_bar0_len=16384 nvme_cap=0x20020103ff nvme_version=0x10400 nvme_command=0x7' "$lane_dir/run.log" || true)" -eq 2 ]] &&
     [[ "$(grep -Ec 'nvme_block_io=1 nvme_block_size=512 nvme_media_present=1 nvme_block_reads=1 nvme_last_block=2047 nvme_lba0_marker=0x4d56454744495242' "$lane_dir/run.log" || true)" -eq 2 ]]; then
    passes=$((passes + 1)); printf '%s\tpass\n' "$lane_name" >> "$OUT/summary.tsv"
  else
    printf '%s\tfail\n' "$lane_name" >> "$OUT/summary.tsv"
    break
  fi
done
[[ "$attempted" -eq "$REQUIRED_LANES" && "$passes" -eq "$REQUIRED_LANES" ]]
