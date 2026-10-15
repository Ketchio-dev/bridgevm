#!/usr/bin/env bash
# T12 submitted inputs remain distinct from generated final-vars outputs.
commit="$(git -C "$REPO" rev-parse HEAD)"
passes=0; attempted=0; gate_asset_hash=absent; binary_hash=absent; vars_hash=absent
input_manifest_hash=absent; image_hash=absent; initial_vars_hash=absent; rust_toolchain=unknown

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
  "input_manifest_sha256": "$input_manifest_hash",
  "expected_initial_image_sha256": "$image_hash",
  "expected_initial_vars_sha256": "$initial_vars_hash",
  "rust_toolchain": "$rust_toolchain",
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
