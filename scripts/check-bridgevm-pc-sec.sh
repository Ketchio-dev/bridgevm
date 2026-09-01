#!/usr/bin/env bash
# Compile and execute deterministic host tests for the freestanding SEC entry.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SEC="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
BUILD="$(mktemp -d "/tmp/bridgevm-pc-sec-test.XXXXXX")"
trap 'rm -rf "$BUILD"' EXIT
grep -Fq '.equ BRIDGE_VM_PC_STACK_TOP,       0x100020000' "$SEC/BridgeVmPcResetVector.S"
grep -Fq '#define BRIDGE_VM_PC_STACK_TOP              0x100020000ULL' "$SEC/BridgeVmPcSec.h"
bash -n "$ROOT/scripts/build-bridgevm-pc-reset-vector.sh"
cc -std=c11 -O2 -Wall -Wextra -Werror -I "$SEC" \
  "$SEC/BridgeVmPcSec.c" "$SEC/BridgeVmPcHob.c" "$ROOT/tests/fixtures/bridgevm-pc-sec-host-test.c" \
  -o "$BUILD/bridgevm-pc-sec-host-test"
"$BUILD/bridgevm-pc-sec-host-test"
echo "PASS: BridgeVM PC SEC rejects corrupt handoffs and builds the bounded PI HOB list"
