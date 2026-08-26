#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CREATE="$ROOT/apps/macos/Sources/BridgeVMControl/CreateVM.swift"
VIEW="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallView.swift"
PRODUCT=("$CREATE" "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineConfig.swift" "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineSession.swift" "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstall.swift")
TESTS="$ROOT/apps/macos/Tests/BridgeVMControlTests"
fail(){ echo "FAIL: $*" >&2; exit 1; }
grep -Fq '3D 드라이버 주입은 서명 provenance 검증기가 없어 사용할 수 없습니다.' "$CREATE" || fail 'unavailable UI message missing'
grep -Fq '차단됨 — 서명 provenance 검증기 없음' "$VIEW" || fail 'legacy request blocker missing'
[[ $(grep -Fc 'injectViogpu3d: false' "$CREATE") -eq 2 ]] || fail 'UI does not force both product flows 3D-off'
! grep -Eq 'Toggle\(".*3D|pickDriverDir|autofillDriverDir|hvfDriverDir|hvfInject' "$CREATE" || fail 'injection control remains'
! grep -Ei 'viogpu3d-injector|hvf-inject|placeholder-nsid1|stageInjection|injectorBuild|bundleInjector|injectorImage|buildingInjector' "${PRODUCT[@]}" || fail 'product injection surface remains'
grep -Fq 'testProductInjectionFailsBeforeSourceInspectionOrVMBundleCreation' "$TESTS/HvfWindowsProductInjectionPreflightTests.swift" || fail 'factory no-mutation test missing'
grep -Fq 'testExistingMarkerAndInjectorCannotAuthorizeProductInjection' "$TESTS/HvfWindowsStaleInjectionMarkerTests.swift" || fail 'stale marker test missing'
grep -Fq 'testPlanValidationBlocksInjectionBeforeFilesystemLookups' "$TESTS/HvfWindowsInstallTests.swift" || fail 'persisted request ordering test missing'
echo 'PASS: Windows-HVF product injection is unavailable and fail-closed'
