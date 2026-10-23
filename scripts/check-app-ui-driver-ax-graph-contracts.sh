#!/bin/bash
# Sourced by check-app-ui-driver-contracts.sh with its bounded output and compile helper.
: "${repo_root:?}" "${scripts:?}" "${tests:?}" "${output:?}"
ax_graph=("$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" "$scripts/AppUIDriverAXRestart.swift")
compile ax "${ax_graph[@]}" "$scripts/AppUIDriverAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-driver-ax-contracts.swift" "$tests/app-ui-driver-ax-overview-contracts.swift" 2>&1 | tee "$output/ax.log"
compile install-cancel-ax "${ax_graph[@]}" "$scripts/AppUIInstallCancelAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-install-cancel-ax-fixture.swift" "$tests/app-ui-install-cancel-ax-refusal-contracts.swift" \
  "$tests/app-ui-install-cancel-ax-contracts.swift" 2>&1 | tee "$output/install-cancel-ax.log"
source "$repo_root/scripts/check-app-ui-driver-ax-restart-contracts.sh"
