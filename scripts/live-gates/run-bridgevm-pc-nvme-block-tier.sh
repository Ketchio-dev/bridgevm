#!/usr/bin/env bash
# T12: fixed N=20 standard UEFI NVMe Block I/O reads on the independent board.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown BridgeVM PC NVMe Block I/O tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" ]] || { echo "BridgeVM PC NVMe Block I/O tier needs --out" >&2; exit 2; }
mkdir -p "$OUT/lanes" "$OUT/artifacts"
readonly REQUIRED_LANES=20
EDK2="${BRIDGEVM_PINNED_EDK2_ROOT:-$HOME/BridgeVM-Workspace/deps/tianocore-edk2-b03a21a}"
commit="$(git -C "$REPO" rev-parse HEAD)"
passes=0; attempted=0; gate_asset_hash=absent; binary_hash=absent; vars_hash=absent

write_receipt() {
  local tier_status="$1" outcome=failed passed=false failures=$((attempted - passes))
  [[ "$tier_status" -eq 0 && "$passes" -eq "$REQUIRED_LANES" ]] && { outcome=completed; passed=true; failures=0; }
  cat > "$OUT/receipt.json" <<EOF
{
  "tier": "t12-bridgevm-pc-nvme-block",
  "gate_id": "bridgevm-pc-standard-uefi-nvme-block-n20",
  "tested_commit": "$commit",
  "commit": "$commit",
  "job_id": "$JOB_ID",
  "host_os": "$(sw_vers -productVersion)",
  "host_hardware": "$(sysctl -n hw.model)",
  "gate_asset_hash": "$gate_asset_hash",
  "binary_hash": "$binary_hash",
  "vars_hash": "$vars_hash",
  "sample_count": $attempted,
  "required_run_count": $REQUIRED_LANES,
  "firmware_boot_count": $((passes * 2)),
  "nvme_register_read_count": $((passes * 4)),
  "nvme_block_io_count": $((passes * 2)),
  "nvme_block_read_count": $((passes * 2)),
  "passes": $passes,
  "failures": $failures,
  "evidence_paths": ["summary.tsv"],
  "known_confounders": [],
  "outcome": "$outcome",
  "pass": $passed,
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
}
finish() {
  local tier_status=$?
  trap - EXIT
  if compgen -G "$OUT/lanes/*/BridgeVmPcVars.fd" >/dev/null; then
    vars_hash="$(find "$OUT/lanes" -name BridgeVmPcVars.fd -type f -print0 | sort -z | xargs -0 shasum -a 256 | sed "s#$OUT/##" | shasum -a 256 | awk '{print $1}')"
  fi
  write_receipt "$tier_status"
  exit "$tier_status"
}
trap finish EXIT

printf 'lane\tresult\n' > "$OUT/summary.tsv"
"$REPO/scripts/build-bridgevm-pc-dxe-entry-firmware.sh" "$EDK2" "$OUT/artifacts"
fd="$OUT/artifacts/BridgeVmPcDxeEntry.fd"
gate_asset_hash="$(shasum -a 256 "$fd" | awk '{print $1}')"
cargo build -q -p bridgevm-hvf --example bridgevm_pc_dxe_entry_live
binary="${CARGO_TARGET_DIR:-$REPO/target}/debug/examples/bridgevm_pc_dxe_entry_live"
codesign --sign - --entitlements "$REPO/apps/macos/HvfRunner.entitlements" --force "$binary"
binary_hash="$(shasum -a 256 "$binary" | awk '{print $1}')"
for lane in $(seq 1 "$REQUIRED_LANES"); do
  lane_name="$(printf 'lane-%02d' "$lane")"; lane_dir="$OUT/lanes/$lane_name"
  mkdir -p "$lane_dir"; attempted=$((attempted + 1))
  if "$REPO/tests/integration/bridgevm-pc-variable-process-persistence.sh" \
      "$binary" "$fd" "$lane_dir" > "$lane_dir/run.log" 2>&1 &&
     [[ "$(grep -Ec 'nvme_bar_reads=2 nvme_bar0=0x[0-9a-f]+ nvme_bar0_len=16384 nvme_cap=0x20020103ff nvme_version=0x10400 nvme_command=0x6' "$lane_dir/run.log" || true)" -eq 2 ]] &&
     [[ "$(grep -Ec 'nvme_block_io=1 nvme_block_size=512 nvme_media_present=1 nvme_block_reads=1 nvme_last_block=2047 nvme_lba0_marker=0x4d56454744495242' "$lane_dir/run.log" || true)" -eq 2 ]]; then
    passes=$((passes + 1)); printf '%s\tpass\n' "$lane_name" >> "$OUT/summary.tsv"
  else
    printf '%s\tfail\n' "$lane_name" >> "$OUT/summary.tsv"
    break
  fi
done
[[ "$attempted" -eq "$REQUIRED_LANES" && "$passes" -eq "$REQUIRED_LANES" ]]
