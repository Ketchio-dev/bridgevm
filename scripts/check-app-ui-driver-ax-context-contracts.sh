#!/bin/bash
# Sourced by check-app-ui-driver-ax-contracts.sh: AX error context and failure locus contracts, values only.
: "${scripts:?}" "${tests:?}" "${output:?}" # Caller owns the output and compile helper.
compile ax-context "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/AppUIInstallCancelProtocolTestSupport.swift" \
  "$tests/app-ui-driver-ax-context-contracts.swift" 2>&1 | tee "$output/ax-context.log"
compile ax-locus-protocol "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/AppUIInstallCancelProtocolTestSupport.swift" \
  "$tests/app-ui-driver-ax-locus-protocol-contracts.swift" "$tests/app-ui-driver-ax-locus-wire-cases.swift" \
  2>&1 | tee "$output/ax-locus-protocol.log"
