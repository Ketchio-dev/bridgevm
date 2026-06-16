#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "SKIP: macOS app start path smoke requires Darwin"
  exit 0
fi

cd "$ROOT"

swift test \
  --package-path "$ROOT/apps/macos" \
  --filter AppSettingsTests/testPersistedDefaultsDriveBundledDaemonSupervisorLaunchEnvironment \
  --jobs 1

swift test \
  --package-path "$ROOT/apps/macos" \
  --filter DashboardViewModelTests/testPerformPrimaryActionStartsStoppedOrErrorVMWhenLaunchReadinessReady \
  --jobs 1

swift test \
  --package-path "$ROOT/apps/macos" \
  --filter DaemonDTOTests/testDaemonClientStartsBackendUsingNameFromListCache \
  --jobs 1

echo "PASS: macOS app start path reaches bundled daemon run_backend spawn"
