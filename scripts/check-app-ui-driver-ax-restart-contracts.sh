#!/bin/bash
: "${scripts:?}" "${tests:?}" "${output:?}" # Caller owns the output and compile helper; injected AX and clocks only.
compile ax-restart "$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" "$scripts/AppUIDriverAXRestart.swift" \
  "$scripts/AppUIDriverAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" "$tests/app-ui-driver-ax-transient-access.swift" \
  "$tests/app-ui-driver-ax-restart-contracts.swift" "$tests/app-ui-driver-ax-restart-terminal-contracts.swift" 2>&1 | tee "$output/ax-restart.log"
