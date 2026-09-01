/** @file
  Build the bounded PI HOB list consumed by the DXE continuation.
  SPDX-License-Identifier: Apache-2.0
**/
#include "BridgeVmPcHob.h"
static void
SetHeader(volatile BRIDGE_VM_PC_HOB_HEADER *Header, uint16_t Type, uint16_t Length)
{
  Header->HobType = Type;
  Header->HobLength = Length;
  Header->Reserved = 0;
}
static void
SetZeroGuid(volatile BRIDGE_VM_PC_GUID *Guid)
{
  uint32_t Index;
  Guid->Data1 = 0;
  Guid->Data2 = 0;
  Guid->Data3 = 0;
  for (Index = 0; Index < 8; ++Index) {
    Guid->Data4[Index] = 0;
  }
}
static void
SetStackGuid(volatile BRIDGE_VM_PC_GUID *Guid)
{
  static const uint8_t Data4[8] = {0x80, 0x7D, 0x52, 0x7B, 0x1D, 0x00, 0xC9, 0xBD};
  uint32_t Index;
  Guid->Data1 = 0x4ED4BF27;
  Guid->Data2 = 0x4092;
  Guid->Data3 = 0x42E9;
  for (Index = 0; Index < 8; ++Index) {
    Guid->Data4[Index] = Data4[Index];
  }
}
static void
BuildHobList(
  const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo,
  volatile void *Buffer
  )
{
  volatile uint8_t *Cursor = (volatile uint8_t *)Buffer;
  volatile BRIDGE_VM_PC_HOB_HANDOFF_INFO *Handoff;
  volatile BRIDGE_VM_PC_HOB_RESOURCE *Resource;
  volatile BRIDGE_VM_PC_HOB_STACK *Stack;
#ifdef BRIDGE_VM_PC_DXE_ENTRY
  volatile BRIDGE_VM_PC_HOB_STACK *PageTables;
#endif
  volatile BRIDGE_VM_PC_HOB_CPU_INFO *Cpu;
  volatile BRIDGE_VM_PC_HOB_HEADER *End;
  uint32_t Index;
  Handoff = (volatile BRIDGE_VM_PC_HOB_HANDOFF_INFO *)Cursor;
  SetHeader(&Handoff->Header, BRIDGE_VM_PC_HOB_HANDOFF, sizeof(*Handoff));
  Handoff->Version = BRIDGE_VM_PC_HOB_VERSION;
  Handoff->BootMode = BRIDGE_VM_PC_BOOT_FULL_CONFIGURATION;
  Handoff->EfiMemoryTop = BootInfo->RamBase + BootInfo->RamSize;
  Handoff->EfiMemoryBottom = BootInfo->RamBase;
  Handoff->EfiFreeMemoryTop = Handoff->EfiMemoryTop;
  Handoff->EfiFreeMemoryBottom = BRIDGE_VM_PC_FREE_MEMORY_BOTTOM;
  Handoff->EfiEndOfHobList = BRIDGE_VM_PC_HOB_LIST_GPA +
                             BRIDGE_VM_PC_HOB_LIST_SIZE - sizeof(*End);
  Cursor += sizeof(*Handoff);
  Resource = (volatile BRIDGE_VM_PC_HOB_RESOURCE *)Cursor;
  SetHeader(&Resource->Header, BRIDGE_VM_PC_HOB_RESOURCE_DESCRIPTOR, sizeof(*Resource));
  SetZeroGuid(&Resource->Owner);
  Resource->ResourceType = BRIDGE_VM_PC_RESOURCE_SYSTEM_MEMORY;
  Resource->ResourceAttribute = BRIDGE_VM_PC_RESOURCE_ATTRIBUTES;
  Resource->PhysicalStart = BootInfo->RamBase;
  Resource->ResourceLength = BootInfo->RamSize;
  Cursor += sizeof(*Resource);
  Stack = (volatile BRIDGE_VM_PC_HOB_STACK *)Cursor;
  SetHeader(&Stack->Header, BRIDGE_VM_PC_HOB_MEMORY_ALLOCATION, sizeof(*Stack));
  SetStackGuid(&Stack->Allocation.Name);
  Stack->Allocation.MemoryBaseAddress = BRIDGE_VM_PC_STACK_BASE;
  Stack->Allocation.MemoryLength = BRIDGE_VM_PC_STACK_SIZE;
  Stack->Allocation.MemoryType = BRIDGE_VM_PC_EFI_BOOT_SERVICES_DATA;
  for (Index = 0; Index < 4; ++Index) {
    Stack->Allocation.Reserved[Index] = 0;
  }
  Cursor += sizeof(*Stack);
#ifdef BRIDGE_VM_PC_DXE_ENTRY
  PageTables = (volatile BRIDGE_VM_PC_HOB_STACK *)Cursor;
  SetHeader(&PageTables->Header, BRIDGE_VM_PC_HOB_MEMORY_ALLOCATION, sizeof(*PageTables));
  SetZeroGuid(&PageTables->Allocation.Name);
  PageTables->Allocation.MemoryBaseAddress = BRIDGE_VM_PC_PAGE_TABLE_BASE;
  PageTables->Allocation.MemoryLength = BRIDGE_VM_PC_PAGE_TABLE_SIZE;
  PageTables->Allocation.MemoryType = BRIDGE_VM_PC_EFI_RESERVED_MEMORY;
  for (Index = 0; Index < 4; ++Index) {
    PageTables->Allocation.Reserved[Index] = 0;
  }
  Cursor += sizeof(*PageTables);
#endif
  Cpu = (volatile BRIDGE_VM_PC_HOB_CPU_INFO *)Cursor;
  SetHeader(&Cpu->Header, BRIDGE_VM_PC_HOB_CPU, sizeof(*Cpu));
  Cpu->SizeOfMemorySpace = BRIDGE_VM_PC_PHYSICAL_ADDRESS_BITS;
  Cpu->SizeOfIoSpace = 0;
  for (Index = 0; Index < 6; ++Index) {
    Cpu->Reserved[Index] = 0;
  }
  Cursor += sizeof(*Cpu);
  End = (volatile BRIDGE_VM_PC_HOB_HEADER *)Cursor;
  SetHeader(End, BRIDGE_VM_PC_HOB_END, sizeof(*End));
}
uint32_t
BridgeVmPcSecMain(
  const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo,
  volatile BRIDGE_VM_PC_SEC_RESULT *Result,
  volatile void *HobList
  )
{
  uint32_t Stage;
  if (Result == (void *)0) {
    return BRIDGE_VM_PC_SEC_BAD_HOB;
  }
  Result->Stage = BRIDGE_VM_PC_SEC_BAD_HOB;
  Result->HobCount = 0;
  Result->HobListGpa = 0;
  Result->HobListSize = 0;
  Result->Reserved = 0;
  if (BootInfo == (void *)0 || HobList == (void *)0) {
    return Result->Stage;
  }
  Stage = BridgeVmPcValidateBootInfo(BootInfo);
  if (Stage == BRIDGE_VM_PC_SEC_SUCCESS) {
    BuildHobList(BootInfo, HobList);
    Result->HobCount = BRIDGE_VM_PC_HOB_COUNT;
    Result->HobListGpa = BRIDGE_VM_PC_HOB_LIST_GPA;
    Result->HobListSize = BRIDGE_VM_PC_HOB_LIST_SIZE;
  }
  Result->Stage = Stage;
  return Stage;
}
