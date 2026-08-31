/** @file
  Minimal Armv8-A stage-1 identity map for BridgeVM PC DXE bring-up.

  SPDX-License-Identifier: Apache-2.0
**/

  .arch armv8-a
  .section .text.mmu, "ax", %progbits
  .global BridgeVmPcEnableMmu
  .type BridgeVmPcEnableMmu, %function

  .equ PAGE_TABLE_BASE, 0x100030000
  .equ NORMAL_BLOCK,    0x701
  .equ DEVICE_BLOCK,    0x0060000000000605
BridgeVmPcEnableMmu:
  stp     x19, x30, [sp, #-16]!
  mov     x19, x0
  movz    x0, #0x0003, lsl #16
  movk    x0, #0x0001, lsl #32
  mov     x1, x0
  movz    x2, #0x3000
1:
  stp     xzr, xzr, [x1], #16
  subs    x2, x2, #16
  b.ne    1b

  add     x1, x0, #0x1000
  add     x2, x0, #0x2000
  orr     x3, x1, #3
  str     x3, [x0]
  orr     x3, x2, #3
  str     x3, [x1]

  movz    x3, #NORMAL_BLOCK
  mov     x4, xzr
  mov     x5, x2
  movz    x6, #64
2:
  orr     x7, x4, x3
  str     x7, [x5], #8
  add     x4, x4, #0x200000
  subs    x6, x6, #1
  b.ne    2b

  movz    x3, #0x0605
  movk    x3, #0x0060, lsl #48
  movz    x4, #0x2000, lsl #16
  add     x5, x2, #0x800
  movz    x6, #48
3:
  orr     x7, x4, x3
  str     x7, [x5], #8
  add     x4, x4, #0x200000
  subs    x6, x6, #1
  b.ne    3b

  movz    x3, #NORMAL_BLOCK
  movz    x4, #0x2600, lsl #16
  orr     x4, x4, x3
  str     x4, [x2, #0x980]

  movz    x3, #0x0605
  movk    x3, #0x0060, lsl #48
  movz    x4, #0x4000, lsl #16
  movz    x5, #3
  movz    x8, #0x4000, lsl #16
  add     x6, x1, #8
4:
  orr     x7, x4, x3
  str     x7, [x6], #8
  add     x4, x4, x8
  subs    x5, x5, #1
  b.ne    4b

  bl      BridgeVmPcMapRam

  movz    x3, #0x0605
  movk    x3, #0x0060, lsl #48
  movz    x4, #0x0020, lsl #32
  add     x6, x1, #0x400
  movz    x5, #128
  movz    x8, #0x4000, lsl #16
5:
  orr     x7, x4, x3
  str     x7, [x6], #8
  add     x4, x4, x8
  subs    x5, x5, #1
  b.ne    5b

  movz    x1, #0x00ff
  msr     mair_el1, x1
  movz    x1, #0x3510
  movk    x1, #0x0080, lsl #16
  movk    x1, #0x0002, lsl #32
  msr     tcr_el1, x1
  msr     ttbr0_el1, x0
  dsb     ish
  tlbi    vmalle1
  dsb     ish
  isb
  mrs     x1, sctlr_el1
  orr     x1, x1, #1
  orr     x1, x1, #4
  orr     x1, x1, #0x1000
  msr     sctlr_el1, x1
  isb
  mov     x0, x19
  ldp     x19, x30, [sp], #16
  ret

  .size BridgeVmPcEnableMmu, . - BridgeVmPcEnableMmu
