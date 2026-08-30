/** @file
  BridgeVM Virtual ARM PC reset entry.

  SPDX-License-Identifier: Apache-2.0
**/

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
  mrs     x4, MPIDR_EL1
  and     x4, x4, #0xffff
  cbnz    x4, SecondaryPark

  movz    x0, #0x2600, lsl #16
  movz    x6, #0x0002, lsl #16
  movk    x6, #0x0001, lsl #32
  mov     sp, x6
  bl      BridgeVmPcSecMain
  movz    x1, #0x1000
  movk    x1, #0x0001, lsl #32
  str     w0, [x1]
  hvc     #0

PrimaryPark:
  wfe
  b       PrimaryPark

SecondaryPark:
  wfe
  b       SecondaryPark

  .size _start, . - _start
