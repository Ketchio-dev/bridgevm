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

_start:
  msr     DAIFSet, #0xf
  mrs     x4, MPIDR_EL1
  and     x4, x4, #0xffff
  cbnz    x4, SecondaryPark

  movz    x0, #0x2600, lsl #16
  ldr     x2, [x0]
  movz    x3, #0x5642
  movk    x3, #0x424d, lsl #16
  movk    x3, #0x4f4f, lsl #32
  movk    x3, #0x3154, lsl #48

  movz    x1, #0x1000
  movk    x1, #0x0001, lsl #32
  cmp     x2, x3
  b.eq    ResultReady
  mov     w5, #2
  b       StoreResult

ResultReady:
  mov     w5, #1

StoreResult:
  str     w5, [x1]
  hvc     #0

PrimaryPark:
  wfe
  b       PrimaryPark

SecondaryPark:
  wfe
  b       SecondaryPark

  .size _start, . - _start
