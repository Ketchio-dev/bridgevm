#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SAFE="$ROOT/scripts/build-hvf-windows-kernel-policy-injector.sh"
GENERIC="$ROOT/scripts/build-hvf-windows-driver-injector.sh"
PLAN="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsProductInjectionPlan.swift"
CREATE="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsVMCreation.swift"
policy="$($SAFE --print-policy)"
for line in source=bundle-private-verified-snapshot activation=offline-dism-only testsigning=disabled firstboot=absent guest-agent=absent overrides=cleared; do grep -Fxq "$line" <<<"$policy"; done
grep -Fq '/usr/bin/env -i PATH=' "$SAFE"
grep -Fq 'ENABLE_TESTSIGNING=0 KERNEL_POLICY_PACKAGE=1' "$SAFE"
grep -Fq 'PLANT_AGENT=0' "$SAFE"
grep -Fq 'bvinject.cmd" "$ASSET_SOURCE/winpeshl-inject.ini' "$SAFE"
! grep -Eq 'bvgpu-firstboot|bcdedit|certutil|BUILD_INJECTOR:-|CHECK_PACKAGE:-' "$SAFE"
grep -Fq '"$KERNEL_POLICY_PACKAGE" != "1"' "$GENERIC"
grep -Fq 'HvfWindowsPreparedPackageSnapshot.bundleRelativePath' "$CREATE" "$PLAN"
grep -Fq 'plan.injectionValidationError() == nil' "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallSessionRun.swift"
echo 'PASS: product kernel-policy injection consumes only a verified offline-DISM snapshot'
