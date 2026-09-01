/** @file
  BridgeVM Virtual ARM PC PI HOB producer contract.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_HOB_H_
#define BRIDGE_VM_PC_HOB_H_
#include "BridgeVmPcSec.h"
#define BRIDGE_VM_PC_RESULT_GPA          0x100001000ULL
#define BRIDGE_VM_PC_HOB_LIST_GPA        0x100004000ULL
#define BRIDGE_VM_PC_STACK_BASE          0x100010000ULL
#define BRIDGE_VM_PC_STACK_SIZE          0x00010000ULL
#ifdef BRIDGE_VM_PC_DXE_ENTRY
#define BRIDGE_VM_PC_PAGE_TABLE_BASE     0x100030000ULL
#define BRIDGE_VM_PC_PAGE_TABLE_SIZE     0x00003000ULL
#define BRIDGE_VM_PC_FREE_MEMORY_BOTTOM  0x100033000ULL
#define BRIDGE_VM_PC_HOB_LIST_SIZE       224U
#define BRIDGE_VM_PC_HOB_COUNT           6U
#else
#define BRIDGE_VM_PC_FREE_MEMORY_BOTTOM  0x100030000ULL
#define BRIDGE_VM_PC_HOB_LIST_SIZE       176U
#define BRIDGE_VM_PC_HOB_COUNT           5U
#endif
#define BRIDGE_VM_PC_HOB_HANDOFF             0x0001U
#define BRIDGE_VM_PC_HOB_MEMORY_ALLOCATION   0x0002U
#define BRIDGE_VM_PC_HOB_RESOURCE_DESCRIPTOR 0x0003U
#define BRIDGE_VM_PC_HOB_CPU                 0x0006U
#define BRIDGE_VM_PC_HOB_END                 0xFFFFU
#define BRIDGE_VM_PC_HOB_VERSION             0x0009U
#define BRIDGE_VM_PC_BOOT_FULL_CONFIGURATION 0U
#define BRIDGE_VM_PC_RESOURCE_SYSTEM_MEMORY  0U
#define BRIDGE_VM_PC_RESOURCE_ATTRIBUTES     0x00002007U
#define BRIDGE_VM_PC_EFI_BOOT_SERVICES_DATA  4U
#define BRIDGE_VM_PC_EFI_RESERVED_MEMORY     0U
#define BRIDGE_VM_PC_PHYSICAL_ADDRESS_BITS   40U
typedef struct {
  uint16_t HobType;
  uint16_t HobLength;
  uint32_t Reserved;
} BRIDGE_VM_PC_HOB_HEADER;
typedef struct {
  uint32_t Data1;
  uint16_t Data2;
  uint16_t Data3;
  uint8_t Data4[8];
} BRIDGE_VM_PC_GUID;
typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  uint32_t Version;
  uint32_t BootMode;
  uint64_t EfiMemoryTop;
  uint64_t EfiMemoryBottom;
  uint64_t EfiFreeMemoryTop;
  uint64_t EfiFreeMemoryBottom;
  uint64_t EfiEndOfHobList;
} BRIDGE_VM_PC_HOB_HANDOFF_INFO;
typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  BRIDGE_VM_PC_GUID Owner;
  uint32_t ResourceType;
  uint32_t ResourceAttribute;
  uint64_t PhysicalStart;
  uint64_t ResourceLength;
} BRIDGE_VM_PC_HOB_RESOURCE;
typedef struct {
  BRIDGE_VM_PC_GUID Name;
  uint64_t MemoryBaseAddress;
  uint64_t MemoryLength;
  uint32_t MemoryType;
  uint8_t Reserved[4];
} BRIDGE_VM_PC_MEMORY_ALLOCATION_HEADER;
typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  BRIDGE_VM_PC_MEMORY_ALLOCATION_HEADER Allocation;
} BRIDGE_VM_PC_HOB_STACK;
typedef struct {
  BRIDGE_VM_PC_HOB_HEADER Header;
  uint8_t SizeOfMemorySpace;
  uint8_t SizeOfIoSpace;
  uint8_t Reserved[6];
} BRIDGE_VM_PC_HOB_CPU_INFO;
typedef struct {
  uint32_t Stage;
  uint32_t HobCount;
  uint64_t HobListGpa;
  uint32_t HobListSize;
  uint32_t Reserved;
} BRIDGE_VM_PC_SEC_RESULT;
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_HEADER) == 8, "PI HOB header size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_HANDOFF_INFO) == 56, "PI PHIT size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_RESOURCE) == 48, "PI resource HOB size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_STACK) == 48, "PI stack HOB size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_CPU_INFO) == 16, "PI CPU HOB size");
_Static_assert(sizeof(BRIDGE_VM_PC_SEC_RESULT) == 24, "SEC result size");
_Static_assert(sizeof(BRIDGE_VM_PC_HOB_HANDOFF_INFO) + sizeof(BRIDGE_VM_PC_HOB_RESOURCE) +
               sizeof(BRIDGE_VM_PC_HOB_STACK) + sizeof(BRIDGE_VM_PC_HOB_CPU_INFO) +
#ifdef BRIDGE_VM_PC_DXE_ENTRY
               sizeof(BRIDGE_VM_PC_HOB_STACK) +
#endif
               sizeof(BRIDGE_VM_PC_HOB_HEADER) == BRIDGE_VM_PC_HOB_LIST_SIZE,
               "PI HOB list size");
_Static_assert(BRIDGE_VM_PC_RESULT_GPA + sizeof(BRIDGE_VM_PC_SEC_RESULT) <=
               BRIDGE_VM_PC_HOB_LIST_GPA, "SEC result overlaps HOB list");
_Static_assert(BRIDGE_VM_PC_HOB_LIST_GPA + BRIDGE_VM_PC_HOB_LIST_SIZE <=
               BRIDGE_VM_PC_STACK_BASE, "HOB list overlaps SEC stack");
_Static_assert(BRIDGE_VM_PC_STACK_BASE + BRIDGE_VM_PC_STACK_SIZE <=
               BRIDGE_VM_PC_FREE_MEMORY_BOTTOM, "SEC stack overlaps free RAM");
#ifdef BRIDGE_VM_PC_DXE_ENTRY
_Static_assert(BRIDGE_VM_PC_PAGE_TABLE_BASE + BRIDGE_VM_PC_PAGE_TABLE_SIZE ==
               BRIDGE_VM_PC_FREE_MEMORY_BOTTOM, "page tables overlap free RAM");
#endif
_Static_assert(BRIDGE_VM_PC_STACK_BASE + BRIDGE_VM_PC_STACK_SIZE ==
               BRIDGE_VM_PC_STACK_TOP, "SEC stack top changed");
uint32_t
BridgeVmPcValidateBootInfo(const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo);
uint32_t
BridgeVmPcSecMain(
  const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo,
  volatile BRIDGE_VM_PC_SEC_RESULT *Result,
  volatile void *HobList
  );
#endif
