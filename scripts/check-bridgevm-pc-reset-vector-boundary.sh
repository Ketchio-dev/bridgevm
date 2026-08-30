#!/usr/bin/env bash
# Deterministic source/ABI guard for the independent reset-vector image.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESET="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
ASM="$RESET/BridgeVmPcResetVector.S"
LINKER="$RESET/BridgeVmPcResetVector.ld"
BOARD="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc.rs"

if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m' "$RESET"; then
  echo "FAIL: reset-vector source references a prohibited compatibility platform" >&2
  exit 1
fi
grep -Fq '.equ BRIDGE_VM_PC_BOOT_INFO_BASE, 0x26000000' "$ASM"
grep -Fq '.equ BRIDGE_VM_PC_RESULT_GPA,      0x100001000' "$ASM"
grep -Fq 'ldr     x2, [x0]' "$ASM"
grep -Fq 'cmp     x2, x3' "$ASM"
grep -Fq 'mov     w5, #1' "$ASM"
grep -Fq 'mov     w5, #2' "$ASM"
grep -Fq 'str     w5, [x1]' "$ASM"
! grep -Eq 'mov[[:space:]]+w2,' "$ASM"
grep -Fq 'ASSERT(_start == 0,' "$LINKER"
grep -Fq 'pub const FLASH_CODE: Region = Region::new(0x0000_0000, 0x0400_0000);' "$BOARD"
grep -Fq 'pub const BOOT_INFO: Region = Region::new(0x2600_0000, 0x1_0000);' "$BOARD"
grep -Fq 'pub const RAM_BASE: u64 = 0x1_0000_0000;' "$BOARD"
bash -n "$ROOT/scripts/build-bridgevm-pc-reset-vector.sh"
echo "PASS: BridgeVM PC reset vector is fixed to the v1 flash, boot-info and RAM contract"
