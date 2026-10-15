#!/bin/bash
: "${scripts:?}" "${tests:?}" "${output:?}" # Caller owns the output and compile helper.
ax_sources=(
  "$scripts/AppUIDriverAX.swift" "$scripts/AppUIDriverAXCalls.swift"
  "$scripts/AppUIDriverAXArrayCalls.swift" "$scripts/AppUIDriverAXRelationships.swift"
  "$scripts/AppUIDriverAXReadiness.swift" "$scripts/AppUIDriverAXTypes.swift"
  "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/app-ui-driver-ax-calls-fixture.swift"
  "$tests/app-ui-driver-ax-relationship-fixture.swift"
)
compile ax-calls "${ax_sources[@]}" "$tests/app-ui-driver-ax-wrapper-boundaries.swift" \
  "$tests/app-ui-driver-ax-calls-contracts.swift" 2>&1 | tee "$output/ax-calls.log"
compile ax-empty "${ax_sources[@]}" "$tests/app-ui-driver-ax-empty-contracts.swift" \
  "$tests/app-ui-driver-ax-count-boundaries.swift" 2>&1 | tee "$output/ax-empty.log"
