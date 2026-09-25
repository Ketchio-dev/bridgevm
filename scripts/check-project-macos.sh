#!/usr/bin/env bash
# Sourced by check-project.sh so step failures accumulate in the parent.
  if command -v swift >/dev/null 2>&1; then
    step "swift build" swift build --package-path apps/macos
    step "native app CLI and runtime" bash scripts/check-native-runtime.sh "$(swift build --package-path apps/macos --show-bin-path)/BridgeVMControl"
    step "swift tests" scripts/run-swift-tests.sh
    step "A9 diagnostic XCTest" swift test --package-path apps/macos --filter 'T17FirstReadyStopCaptureTests|HvfRuntimeDiagnosticStopAdmissionTests'
    step "native UI driver contracts" scripts/check-app-ui-driver-contracts.sh --output "$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-ui-driver.XXXXXX")/contracts"
    step "xctest shim suites" scripts/run-xctest-shim-suites.sh
    step "release executable boundaries" bash -c 'scripts/check-release-overrides.sh && tests/integration/hvf-runner-release-exec-boundary-smoke.sh && tests/integration/hvf-runner-release-profile-override-smoke.sh'
  else
    printf '\nswift toolchain absent: skipping macOS app checks\n' >&2
  fi
