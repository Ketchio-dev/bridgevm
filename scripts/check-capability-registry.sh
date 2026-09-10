#!/usr/bin/env bash
set -euo pipefail
python3 tests/integration/capability-freshness-smoke.py
python3 scripts/render-capability-status.py --check
