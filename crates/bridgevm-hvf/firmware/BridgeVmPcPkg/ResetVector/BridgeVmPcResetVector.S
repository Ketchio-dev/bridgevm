// BridgeVM Virtual ARM PC reset entry. SPDX-License-Identifier: Apache-2.0
  .arch armv8-a
  .section .text.reset, "ax", %progbits
  .balign 4
  .global _start
  .type _start, %function
  .equ BRIDGE_VM_PC_BOOT_INFO_BASE, 0x26000000
  .equ BRIDGE_VM_PC_RESULT_GPA,      0x100001000
  .equ BRIDGE_VM_PC_STACK_TOP,       0x100020000
_start:
  msr     DAIFSet, #0xf
#ifdef BRIDGE_VM_PC_DXE_ENTRY
  bl      BridgeVmPcInstallExceptionVector
#endif
  mrs     x4, MPIDR_EL1
  and     x4, x4, #0xffff
  cbnz    x4, SecondaryPark
#ifdef BRIDGE_VM_PC_DXE_ENTRY
  movz    x6, #0x0002, lsl #16
  movk    x6, #0x0001, lsl #32
  mov     sp, x6
  bl      BridgeVmPcEnableMmu
#endif
  movz    x0, #0x2600, lsl #16
  movz    x1, #0x1000
  movk    x1, #0x0001, lsl #32
  movz    x2, #0x4000
  movk    x2, #0x0001, lsl #32
#ifndef BRIDGE_VM_PC_DXE_ENTRY
  movz    x6, #0x0002, lsl #16
  movk    x6, #0x0001, lsl #32
  mov     sp, x6
#endif
  bl      BridgeVmPcSecMain
#ifdef BRIDGE_VM_PC_DXE_ENTRY
  movz    x0, #0x1000
  movk    x0, #0x0001, lsl #32
  movz    x1, #0x4000
  movk    x1, #0x0001, lsl #32
  bl      BridgeVmPcDxeIplMain
  dsb     sy
  hvc     #2
#else
  hvc     #0
#endif
PrimaryPark:
  wfe
  b       PrimaryPark
SecondaryPark:
  wfe
  b       SecondaryPark
