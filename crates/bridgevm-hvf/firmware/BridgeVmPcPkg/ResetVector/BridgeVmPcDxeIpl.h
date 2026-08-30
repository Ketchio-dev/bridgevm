/** @file
  Bounded BridgeVM PC DXE Core handoff contract.

  SPDX-License-Identifier: Apache-2.0
**/

#ifndef BRIDGE_VM_PC_DXE_IPL_H_
#define BRIDGE_VM_PC_DXE_IPL_H_

#include "BridgeVmPcHob.h"

#define BRIDGE_VM_PC_DXE_FV_GPA          0x00100000ULL
#define BRIDGE_VM_PC_DXE_FV_SIZE         0x00100000ULL
#define BRIDGE_VM_PC_DXE_CORE_FFS_OFFSET 0x00000078U
#define BRIDGE_VM_PC_DXE_CORE_PE_OFFSET  0x00000094U
#define BRIDGE_VM_PC_DXE_CORE_LOAD_BASE  0x100400000ULL
#define BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE 0x00017000U
#define BRIDGE_VM_PC_DXE_CORE_ENTRY      0x100406BECULL
#define BRIDGE_VM_PC_DXE_HOB_LIST_SIZE   272U
#define BRIDGE_VM_PC_DXE_HOB_COUNT       7U

#define BRIDGE_VM_PC_SEC_BAD_FV          8U
#define BRIDGE_VM_PC_SEC_BAD_DXE_IMAGE   9U
#define BRIDGE_VM_PC_SEC_DXE_RETURNED     10U
#define BRIDGE_VM_PC_HOB_FV              0x0005U
#define BRIDGE_VM_PC_EFI_BOOT_SERVICES_CODE 3U

typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  uint64_t BaseAddress;
  uint64_t Length;
} BRIDGE_VM_PC_HOB_FV_INFO;

typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  BRIDGE_VM_PC_MEMORY_ALLOCATION_HEADER Allocation;
  BRIDGE_VM_PC_GUID ModuleName;
  uint64_t EntryPoint;
} BRIDGE_VM_PC_HOB_MODULE;

_Static_assert(sizeof(BRIDGE_VM_PC_HOB_FV_INFO) == 24, "PI FV HOB size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_MODULE) == 72, "PI module HOB size");
_Static_assert(BRIDGE_VM_PC_HOB_LIST_SIZE + sizeof(BRIDGE_VM_PC_HOB_FV_INFO) +
               sizeof(BRIDGE_VM_PC_HOB_MODULE) == BRIDGE_VM_PC_DXE_HOB_LIST_SIZE,
               "DXE HOB list size");
_Static_assert(BRIDGE_VM_PC_DXE_CORE_LOAD_BASE + BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE <
               BRIDGE_VM_PC_RAM_BASE + (512ULL << 20), "DXE Core outside probe RAM");

uint32_t
BridgeVmPcDxeIplMain(
  volatile BRIDGE_VM_PC_SEC_RESULT *Result,
  volatile void *HobList
  );

#endif
