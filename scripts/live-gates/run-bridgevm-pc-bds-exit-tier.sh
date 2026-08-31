#!/usr/bin/env bash
# T13: fixed N=20 UEFI BDS loads an ESP app and reaches post-ExitBootServices.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown BridgeVM PC BDS tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" ]] || { echo "BridgeVM PC BDS tier needs --out" >&2; exit 2; }
mkdir -p "$OUT/lanes" "$OUT/artifacts"
readonly REQUIRED_LANES=20
EDK2="${BRIDGEVM_PINNED_EDK2_ROOT:-$HOME/BridgeVM-Workspace/deps/tianocore-edk2-b03a21a}"
commit="$(git -C "$REPO" rev-parse HEAD)"
passes=0; attempted=0; gate_asset_hash=absent; image_sha256=absent
binary_hash=absent; vars_hash=absent

write_receipt() {
  local tier_status="$1" outcome=failed passed=false failures=$((attempted - passes))
  [[ "$tier_status" -eq 0 && "$passes" -eq "$REQUIRED_LANES" ]] && {
    outcome=completed; passed=true; failures=0;
  }
  cat > "$OUT/receipt.json" <<EOF
{
  "tier": "t13-bridgevm-pc-bds-exit",
  "gate_id": "bridgevm-pc-uefi-bds-exit-boot-services-n20",
  "tested_commit": "$commit",
  "commit": "$commit",
  "job_id": "$JOB_ID",
  "host_os": "$(sw_vers -productVersion)",
  "host_hardware": "$(sysctl -n hw.model)",
  "gate_asset_hash": "$gate_asset_hash",
  "image_sha256": "$image_sha256",
  "binary_hash": "$binary_hash",
  "vars_hash": "$vars_hash",
  "sample_count": $attempted,
  "required_run_count": $REQUIRED_LANES,
  "run_count": $passes,
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
  if compgen -G "$OUT/lanes/*/BridgeVmPcBootVars.fd" >/dev/null; then
    vars_hash="$(find "$OUT/lanes" -name BridgeVmPcBootVars.fd -type f -print0 |
      sort -z | xargs -0 shasum -a 256 | sed "s#$OUT/##" |
      shasum -a 256 | awk '{print $1}')"
  fi
  write_receipt "$tier_status"
  exit "$tier_status"
}
trap finish EXIT

printf 'lane\tresult\n' > "$OUT/summary.tsv"
"$REPO/scripts/build-bridgevm-pc-boot-firmware.sh" "$EDK2" "$OUT/artifacts"
fd="$OUT/artifacts/BridgeVmPcBoot.fd"
media="$OUT/artifacts/BridgeVmPcBoot.img"
vars="$OUT/artifacts/BridgeVmPcBootVars.fd"
gate_asset_hash="$(shasum -a 256 "$fd" | awk '{print $1}')"
image_sha256="$(shasum -a 256 "$media" | awk '{print $1}')"
cargo build -q --release -p bridgevm-hvf --example bridgevm_pc_boot_live
binary="${CARGO_TARGET_DIR:-$REPO/target}/release/examples/bridgevm_pc_boot_live"
codesign --sign - --entitlements "$REPO/apps/macos/HvfRunner.entitlements" --force "$binary"
binary_hash="$(shasum -a 256 "$binary" | awk '{print $1}')"
for lane in $(seq 1 "$REQUIRED_LANES"); do
  lane_name="$(printf 'lane-%02d' "$lane")"; lane_dir="$OUT/lanes/$lane_name"
  mkdir -p "$lane_dir"
  lane_media="$lane_dir/BridgeVmPcBoot.img"
  lane_vars="$lane_dir/BridgeVmPcBootVars.fd"
  cp -c "$media" "$lane_media"
  cp -c "$vars" "$lane_vars"
  [[ "$(stat -f %i "$lane_media")" != "$(stat -f %i "$media")" ]]
  [[ "$(stat -f %i "$lane_vars")" != "$(stat -f %i "$vars")" ]]
  attempted=$((attempted + 1))
  if "$binary" "$fd" "$lane_media" "$lane_vars" > "$lane_dir/run.log" 2>&1 &&
     [[ "$(grep -Fc 'BridgeVM Virtual ARM PC BDS/ESP/PE/ExitBootServices probe: PASS' "$lane_dir/run.log")" -eq 1 ]] &&
     [[ "$(grep -Ec '^stage=11 arch=0xfff filesystems=1 image=0x[0-9a-f]+\+0x[0-9a-f]+$' "$lane_dir/run.log")" -eq 1 ]] &&
     [[ "$(grep -Ec 'exit_boot_services_attempts=[1-3] ' "$lane_dir/run.log")" -eq 1 ]] &&
     [[ "$(grep -Fc 'LIVE PROOF: DXE Core invoked BDS, which loaded BOOTAA64.EFI from NVMe and reached code after ExitBootServices' "$lane_dir/run.log")" -eq 1 ]] &&
     [[ "$(stat -f %z "$lane_media")" -eq 67108864 ]] &&
     [[ "$(stat -f %z "$lane_vars")" -eq 65536 ]] &&
     [[ "$(shasum -a 256 "$lane_media" | awk '{print $1}')" == "$image_sha256" ]]; then
    passes=$((passes + 1)); printf '%s\tpass\n' "$lane_name" >> "$OUT/summary.tsv"
  else
    printf '%s\tfail\n' "$lane_name" >> "$OUT/summary.tsv"
    break
  fi
done
[[ "$attempted" -eq "$REQUIRED_LANES" && "$passes" -eq "$REQUIRED_LANES" ]]
