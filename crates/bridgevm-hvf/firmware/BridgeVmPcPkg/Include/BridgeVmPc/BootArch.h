/** @file
  BridgeVM Virtual ARM PC required architectural-protocol evidence mask.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_BOOT_ARCH_H_
#define BRIDGE_VM_PC_BOOT_ARCH_H_

#define BRIDGE_VM_PC_ARCH_SECURITY       (1ULL << 0)
#define BRIDGE_VM_PC_ARCH_CPU            (1ULL << 1)
#define BRIDGE_VM_PC_ARCH_METRONOME      (1ULL << 2)
#define BRIDGE_VM_PC_ARCH_TIMER          (1ULL << 3)
#define BRIDGE_VM_PC_ARCH_WATCHDOG       (1ULL << 4)
#define BRIDGE_VM_PC_ARCH_RUNTIME        (1ULL << 5)
#define BRIDGE_VM_PC_ARCH_VARIABLE       (1ULL << 6)
#define BRIDGE_VM_PC_ARCH_VARIABLE_WRITE (1ULL << 7)
#define BRIDGE_VM_PC_ARCH_CAPSULE        (1ULL << 8)
#define BRIDGE_VM_PC_ARCH_MONOTONIC      (1ULL << 9)
#define BRIDGE_VM_PC_ARCH_RESET          (1ULL << 10)
#define BRIDGE_VM_PC_ARCH_RTC            (1ULL << 11)
#define BRIDGE_VM_PC_ARCH_REQUIRED       ((1ULL << 12) - 1ULL)

#endif
