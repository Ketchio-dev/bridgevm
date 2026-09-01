#!/usr/bin/env bash
# Deterministic source guard for the independent-board BDS/ESP/EBS boundary.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
BDS="$PKG/Drivers/BootManagerDxe/BootManagerDxe.c"
APP="$PKG/Applications/ExitBootServicesProbe/ExitBootServicesProbe.c"
BUILD="$ROOT/scripts/build-bridgevm-pc-boot-firmware.sh"

grep -Fq 'EfiEventGroupSignal (&gEfiEndOfDxeEventGroupGuid)' "$BDS"
grep -Fq 'gBS->LoadImage (TRUE, mImageHandle' "$BDS"
grep -Fq 'EfiSignalEventReadyToBoot ()' "$BDS"
grep -Fq 'BridgeVmPcStartImageAndRecord (BootImage)' "$BDS" && grep -Fq 'gBS->StartImage (ImageHandle' "$PKG/Drivers/BootManagerDxe/StartImageFailure.c"
grep -Fq 'GetMemoryMap' "$APP"
grep -Fq 'ExitBootServices' "$APP"
grep -Fq 'BRIDGE_VM_PC_BOOT_STAGE_POST_EXIT' "$APP"
grep -Fq 'b03a21a63e3bd001f52c527e5a57feddb53a690b' "$BUILD"
grep -Fq 'a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979' "$BUILD"
grep -Fq '71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063' "$BUILD"
awk '/^\[Packages\]/{inside=1;next} /^\[/{inside=0} inside && /Pkg\/.*\.dec/{print $1}' \
  "$PKG/Drivers/BootManagerDxe/BootManagerDxe.inf" \
  "$PKG/Applications/ExitBootServicesProbe/ExitBootServicesProbe.inf" |
  sort -u | diff -u - <(printf '%s\n' BridgeVmPcPkg/BridgeVmPcPkg.dec MdePkg/MdePkg.dec)
bash -n "$ROOT/scripts/build-bridgevm-pc-boot-modules.sh" \
  "$ROOT/scripts/build-bridgevm-pc-boot-fv.sh" \
  "$ROOT/scripts/build-bridgevm-pc-boot-firmware.sh" \
  "$ROOT/scripts/live-gates/run-bridgevm-pc-bds-exit-tier.sh"
python3 -m py_compile "$ROOT/scripts/build-bridgevm-pc-boot-media.py" \
  "$ROOT/scripts/check-bridgevm-pc-boot-fv.py"
python3 "$ROOT/scripts/build-bridgevm-pc-boot-media.py" --self-test
"$ROOT/tests/integration/bridgevm-pc-bds-exit-live-tier-smoke.sh"
echo "PASS: BridgeVM PC BDS/ESP/ExitBootServices boundary is fail-closed"
