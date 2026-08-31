/** @file
  Bounded RAM identity-map population for the BridgeVM PC stage-1 table.

  SPDX-License-Identifier: Apache-2.0
**/

  .arch armv8-a
  .section .text.mmu_ram, "ax", %progbits
  .global BridgeVmPcMapRam
  .type BridgeVmPcMapRam, %function

  .equ RAM_SIZE_OFFSET,  96
  .equ NORMAL_BLOCK,     0x701
  .equ RAM_FIRST_GIB,    4
  .equ RAM_MAX_BLOCKS,   124

// x1 is the L1 table and x19 is the bounded boot-info pointer. The later SEC
// validation remains authoritative; the clamp prevents malformed input from
// writing outside the single L1 table before that validation can run.
BridgeVmPcMapRam:
  ldr     x2, [x19, #RAM_SIZE_OFFSET]
  lsr     x6, x2, #30
  and     x2, x2, #0x3fffffff
  cmp     x2, #0
  cinc    x6, x6, ne
  movz    x2, #1
  cmp     x6, x2
  csel    x6, x6, x2, hs
  movz    x2, #RAM_MAX_BLOCKS
  cmp     x6, x2
  csel    x6, x6, x2, ls

  movz    x3, #NORMAL_BLOCK
  movz    x4, #0x0001, lsl #32
  add     x5, x1, #(RAM_FIRST_GIB * 8)
  movz    x8, #0x4000, lsl #16
1:
  orr     x7, x4, x3
  str     x7, [x5], #8
  add     x4, x4, x8
  subs    x6, x6, #1
  b.ne    1b
  ret

  .size BridgeVmPcMapRam, . - BridgeVmPcMapRam
