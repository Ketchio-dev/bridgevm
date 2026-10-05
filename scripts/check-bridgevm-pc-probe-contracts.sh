#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bash "$ROOT/scripts/check-bridgevm-pc-reset-vector-boundary.sh"
bash "$ROOT/scripts/check-bridgevm-pc-boot-boundary.sh"
python3 "$ROOT/tests/integration/bridgevm-pc-firmware-pins-contract.py"
python3 "$ROOT/tests/integration/bridgevm-pc-pci-identities-contract.py"
