#!/usr/bin/env bash
# Deterministic source/ABI guard for the independent reset-vector image.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESET="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
ASM="$RESET/BridgeVmPcResetVector.S"
SEC="$RESET"
LINKER="$RESET/BridgeVmPcResetVector.ld"
BOARD="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc.rs"
if grep -R -n -i -E 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m' "$RESET"; then
  echo "FAIL: reset-vector source references a prohibited compatibility platform" >&2
  exit 1
fi
grep -Fq '.equ BRIDGE_VM_PC_BOOT_INFO_BASE, 0x26000000' "$ASM"
grep -Fq '.equ BRIDGE_VM_PC_RESULT_GPA,      0x100001000' "$ASM"
grep -Fq 'mov     sp, x6' "$ASM"
awk '/movz    x1, #0x1000/ {result=1} /movz    x2, #0x4000/ {hob=1} /bl      BridgeVmPcSecMain/ {called=result && hob} END {exit !called}' "$ASM"
! grep -Fq 'str     w0, [x1]' "$ASM"
grep -Fq '_Static_assert(sizeof(BRIDGE_VM_PC_BOOT_INFO) == BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE' "$SEC/BridgeVmPcSec.h"
grep -Fq 'if (!HeaderChecksumIsZero(BootInfo))' "$SEC/BridgeVmPcSec.c"
grep -Fq 'BootInfo->RamSize < BRIDGE_VM_PC_FREE_MEMORY_BOTTOM' "$SEC/BridgeVmPcSec.c"
grep -Fq 'ASSERT(_start == 0,' "$LINKER"
grep -Fq 'pub const FLASH_CODE: Region = Region::new(0x0000_0000, 0x0400_0000);' "$BOARD"
grep -Fq 'pub const BOOT_INFO: Region = Region::new(0x2600_0000, 0x1_0000);' "$BOARD"
grep -Fq 'pub const RAM_BASE: u64 = 0x1_0000_0000;' "$BOARD"
"$ROOT/scripts/check-bridgevm-pc-sec.sh"
"$ROOT/scripts/check-bridgevm-pc-mmu.sh"
echo "PASS: BridgeVM PC reset vector is fixed to the v1 flash, boot-info and RAM contract"
