#!/bin/bash
# Sourced by check-app-ui-driver-ax-contracts.sh: AX error context, then failure locus, contracts; no AX message.
: "${repo_root:?}" "${scripts:?}" "${tests:?}" "${output:?}" # Caller owns the output and compile helper.
compile ax-context "$tests/AppUIDriverProtocolTestSupport.swift" "$tests/AppUIInstallCancelProtocolTestSupport.swift" \
  "$tests/app-ui-driver-ax-context-contracts.swift" 2>&1 | tee "$output/ax-context.log"
source "$repo_root/scripts/check-app-ui-driver-ax-locus-contracts.sh"
