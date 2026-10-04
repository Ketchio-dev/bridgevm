#!/usr/bin/env bash
# Owned synthetic queue only; hardware/provider substitutes never become receipts.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 tests/integration/t22-pair-queue-input-contract.py
python3 tests/integration/t22-pair-queue-submission-contract.py
python3 tests/integration/t22-pair-queue-process-contract.py
python3 tests/integration/t22-pair-worker-queue-contract.py && python3 tests/integration/t22-pair-source-boundary-contract.py && python3 tests/integration/t22-pair-cache-boundary-contract.py && python3 tests/integration/t22-pair-cache-alias-contract.py && python3 tests/integration/t22-pair-archive-cache-contract.py && python3 tests/integration/t22-pair-archive-admission-contract.py && python3 tests/integration/t22-pair-bootstrap-isolation-contract.py
