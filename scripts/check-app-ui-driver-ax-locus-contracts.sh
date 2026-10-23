#!/bin/bash
# Sourced by check-app-ui-driver-ax-context-contracts.sh: the AX failure locus on the wire and in the driver.
: "${scripts:?}" "${tests:?}" "${output:?}" # Caller owns the output and compile helper; values and injected AX only.
compile ax-locus-protocol "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/AppUIInstallCancelProtocolTestSupport.swift" \
  "$tests/app-ui-driver-ax-locus-protocol-contracts.swift" "$tests/app-ui-driver-ax-locus-wire-cases.swift" \
  2>&1 | tee "$output/ax-locus-protocol.log"
# The driver's locus recorder over injected graphs, and over the production wrapper with injected messaging.
ax_locus=("$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" "$scripts/AppUIDriverAXRestart.swift"
  "$scripts/AppUIDriverAXOperations.swift" "$scripts/AppUIDriverAXLocusAccess.swift" "$scripts/AppUIDriverAX.swift"
  "$scripts/AppUIDriverAXCalls.swift" "$scripts/AppUIDriverAXArrayCalls.swift" "$scripts/AppUIDriverAXRelationships.swift"
  "$scripts/AppUIDriverAXReadiness.swift")
compile ax-locus "${ax_locus[@]}" "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-driver-ax-calls-fixture.swift" "$tests/app-ui-driver-ax-relationship-fixture.swift" \
  "$tests/app-ui-driver-ax-locus-fake.swift" "$tests/app-ui-driver-ax-locus-contracts.swift" \
  "$tests/app-ui-driver-ax-locus-sweep-contracts.swift" "$tests/app-ui-driver-ax-locus-names-contracts.swift" \
  2>&1 | tee "$output/ax-locus.log"
