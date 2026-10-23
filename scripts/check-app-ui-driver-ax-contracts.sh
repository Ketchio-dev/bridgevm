#!/bin/bash
: "${repo_root:?}" "${scripts:?}" "${tests:?}" "${output:?}" # Caller supplies the owned output and compile helper.
source "$repo_root/scripts/check-app-ui-driver-ax-graph-contracts.sh"
source "$repo_root/scripts/check-app-ui-driver-ax-context-contracts.sh"
source "$repo_root/scripts/check-app-ui-driver-ax-call-contracts.sh"
source "$repo_root/scripts/check-app-ui-driver-ax-service-contracts.sh"
