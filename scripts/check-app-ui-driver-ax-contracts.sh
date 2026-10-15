#!/bin/bash
: "${repo_root:?}" "${scripts:?}" "${tests:?}" "${output:?}" # Caller supplies the owned output and compile helper.
source "$repo_root/scripts/check-app-ui-driver-ax-graph-contracts.sh"
compile ax-context "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/AppUIInstallCancelProtocolTestSupport.swift" \
  "$tests/app-ui-driver-ax-context-contracts.swift" 2>&1 | tee "$output/ax-context.log"
compile ax-calls "$scripts/AppUIDriverAX.swift" "$scripts/AppUIDriverAXCalls.swift" \
  "$scripts/AppUIDriverAXReadiness.swift" "$scripts/AppUIDriverAXTypes.swift" \
  "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/app-ui-driver-ax-calls-fixture.swift" \
  "$tests/app-ui-driver-ax-calls-contracts.swift" 2>&1 | tee "$output/ax-calls.log"
source "$repo_root/scripts/check-app-ui-driver-ax-service-contracts.sh"
