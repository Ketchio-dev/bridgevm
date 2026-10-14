#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-nvme-block-tier.sh"
grep -q 'REQUIRED_LANES=20' "$RUN"
grep -q 'bridgevm-pc-standard-uefi-nvme-block-n20' "$REPO/scripts/live-gates/t12-receipt.sh"
[[ "$(grep -Ec 'nvme_command=0x7|nvme_block_io=1 nvme_block_size=512' "$RUN")" -eq 2 ]]
! grep -Eq 'cargo build|codesign --sign|build-bridgevm-pc-dxe-entry-firmware' "$RUN"
for contract in admission-contract input-refusals runtime-contract worker-contract; do
  python3 "$REPO/tests/integration/t12-sealed-$contract.py"
done
echo "bridgevm pc nvme block live tier smoke: PASS (synthetic sealed-input contracts only)"
