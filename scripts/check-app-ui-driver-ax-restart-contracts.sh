#!/bin/bash
: "${scripts:?}" "${tests:?}" "${output:?}" "${ax_graph[0]:?}" # The graph script owns output, compile and sources; injected AX and clocks only.
compile ax-restart "${ax_graph[@]}" "$scripts/AppUIDriverAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-driver-ax-transient-access.swift" "$tests/AppUIDriverProtocolTestSupport.swift" \
  "$tests"/app-ui-driver-ax-restart*-contracts.swift 2>&1 | tee "$output/ax-restart.log"
