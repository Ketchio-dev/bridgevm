#!/bin/bash
: "${scripts:?}" "${tests:?}" "${output:?}" # Actual services; fixture calls only pure error mapping methods.
compile ax-service "$scripts"/AppUIDriver*.swift "$scripts"/AppUIInstallCancel*.swift \
  "$scripts"/AppUIHostV2*.swift "$scripts/AppUIHostLaunchOwnership.swift" "$scripts/LaunchAppUIHost.swift" \
  "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/app-ui-driver-ax-service-contracts.swift" \
  2>&1 | tee "$output/ax-service.log"
