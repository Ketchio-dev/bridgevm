#!/usr/bin/env bash
# The existing Apple XCTest selection plus its real, harmless fixture helpers.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
source "$ROOT/scripts/prepare-native-test-helpers.sh"
swift test --package-path apps/macos --filter 'FirstRun|HvfAppUI|HvfCurrentGenerationImport|HvfMediaImport|HvfWindowsImport|HvfWindowsInstall|HvfProductCPUValidation|LibraryModelPersistence|HvfRuntime|HvfOwnedRuntime|HvfProductBackendConfiguration|NativeCLI|NativeLibraryReader|NativeRuntime'
