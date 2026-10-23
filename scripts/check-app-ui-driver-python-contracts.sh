#!/bin/bash
# Sourced by the driver contract entry point with its admitted output directory.
: "${tests:?}" "${output:?}"
python3 "$tests/app-ui-driver-packaging-contracts.py" 2>&1 | tee "$output/packaging.log"
for suite in app-ui-signing-contracts app-ui-host-v2-tier-smoke app-ui-host-model-creations-contract app-ui-install-cancel-tier-smoke app-ui-supervisor-bundle-contract; do
  python3 "$tests/$suite.py" 2>&1 | tee "$output/$suite.log"
done
