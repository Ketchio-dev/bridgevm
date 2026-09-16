#!/usr/bin/env bash
# Sourced by check-project.sh so step failures accumulate in the parent.
  # --- macOS app -------------------------------------------------------------
  if command -v swift >/dev/null 2>&1; then
    step "swift build" swift build --package-path apps/macos
    step "native app CLI" python3 tests/integration/native-app-cli-contract.py "$(swift build --package-path apps/macos --show-bin-path)/BridgeVMControl"
    step "swift tests" scripts/run-swift-tests.sh
    step "native UI driver contracts" scripts/check-app-ui-driver-contracts.sh --output "$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-ui-driver.XXXXXX")/contracts"
    step "xctest shim suites" scripts/run-xctest-shim-suites.sh
    step "release overrides" scripts/check-release-overrides.sh
  else
    printf '\nswift toolchain absent: skipping macOS app checks\n' >&2
  fi
