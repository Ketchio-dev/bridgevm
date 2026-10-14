#!/bin/bash
# Sourced by check-app-ui-driver-contracts.sh with its bounded output and compile helper.
: "${scripts:?}" "${tests:?}" "${output:?}"
compile ax "$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" \
  "$scripts/AppUIDriverAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-driver-ax-contracts.swift" 2>&1 | tee "$output/ax.log"
compile install-cancel-ax "$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" \
  "$scripts/AppUIInstallCancelAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" "$tests/app-ui-install-cancel-ax-fixture.swift" \
  "$tests/app-ui-install-cancel-ax-refusal-contracts.swift" "$tests/app-ui-install-cancel-ax-contracts.swift" \
  2>&1 | tee "$output/install-cancel-ax.log"
