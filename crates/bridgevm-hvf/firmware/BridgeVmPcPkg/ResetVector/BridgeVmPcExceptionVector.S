/** @file
  Fail-closed EL1 exception trap for the BridgeVM PC DXE bring-up probe.

  SPDX-License-Identifier: Apache-2.0
**/

  .arch armv8-a
  .section .text.exception, "ax", %progbits
  .global BridgeVmPcInstallExceptionVector
  .type BridgeVmPcInstallExceptionVector, %function
BridgeVmPcInstallExceptionVector:
  mrs     x7, cpacr_el1
  orr     x7, x7, #0x300000
  msr     cpacr_el1, x7
  adrp    x7, BridgeVmPcExceptionVectors
  add     x7, x7, :lo12:BridgeVmPcExceptionVectors
  msr     vbar_el1, x7
  isb
  ret

  .global BridgeVmPcExceptionTrap
  .type BridgeVmPcExceptionTrap, %function
BridgeVmPcExceptionTrap:
  mrs     x0, esr_el1
  mrs     x1, elr_el1
  mrs     x2, far_el1
  hvc     #3
  b       BridgeVmPcExceptionTrap

  .section .vectors, "ax", %progbits
  .balign 2048
  .global BridgeVmPcExceptionVectors
BridgeVmPcExceptionVectors:
  .rept 16
  b       BridgeVmPcExceptionTrap
  .space 124
  .endr
