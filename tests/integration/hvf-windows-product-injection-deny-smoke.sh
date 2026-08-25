#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CREATE="$ROOT/apps/macos/Sources/BridgeVMControl/CreateVM.swift"
VIEW="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallView.swift"
TESTS="$ROOT/apps/macos/Tests/BridgeVMControlTests"
fail(){ echo "FAIL: $*" >&2; exit 1; }
grep -Fq 'HvfWindowsPreparedPackageSnapshot' "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsVMCreation.swift" || fail 'verified snapshot is not consumed'
! grep -Ei 'placeholder-nsid1|hvf-inject-pending' "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineConfig.swift" "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineSession.swift" || fail 'legacy runtime authorization returned'
grep -Fq 'testProductInjectionFailsBeforeSourceInspectionOrVMBundleCreation' "$TESTS/HvfWindowsProductInjectionPreflightTests.swift" || fail 'factory no-mutation test missing'
grep -Fq 'testExistingMarkerAndInjectorCannotAuthorizeProductInjection' "$TESTS/HvfWindowsStaleInjectionMarkerTests.swift" || fail 'stale marker test missing'
grep -Fq 'testPlanValidationBlocksInjectionBeforeFilesystemLookups' "$TESTS/HvfWindowsInstallTests.swift" || fail 'persisted request ordering test missing'
python3 "$ROOT/scripts/sign-hvf-windows-kernel-policy-package.py" --self-test
"$ROOT/tests/integration/hvf-windows-kernel-policy-injection-smoke.sh"
echo 'PASS: Windows-HVF product injection provenance and legacy-marker denial are fail-closed'
