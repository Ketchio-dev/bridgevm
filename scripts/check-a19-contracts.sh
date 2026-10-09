#!/usr/bin/env bash
# Deterministic A19 receipt, interruption and lifecycle contracts; no live guest.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 tests/integration/a19-quota-refusal-live-tier-contract.py; python3 tests/integration/a19-archived-receipt-read-contract.py
python3 tests/integration/a19-interrupted-restore-live-tier-contract.py
python3 tests/integration/a19-interrupted-restore-public-contract.py
python3 tests/integration/a19-interrupted-restore-guest-share-contract.py
python3 tests/integration/a19-interrupted-restore-cases-contract.py
python3 tests/integration/a19-interrupted-restore-case-hashes-contract.py
python3 tests/integration/a19-lifecycle-campaign-receipt-contract.py; python3 tests/integration/a19-lifecycle-campaign-dispatch-contract.py; python3 tests/integration/a19-contract-entrypoint-contract.py
python3 tests/integration/a19-lifecycle-campaign-runner-contract.py
python3 tests/integration/a19-lifecycle-campaign-progress-contract.py
python3 tests/integration/a19-interrupted-restore-auxiliary-orchestration-contract.py
python3 tests/integration/a19-interrupted-restore-production-collection-contract.py
python3 tests/integration/a19-interrupted-restore-process-cleanup-contract.py && scripts/check-t22-pair-preparation.sh
