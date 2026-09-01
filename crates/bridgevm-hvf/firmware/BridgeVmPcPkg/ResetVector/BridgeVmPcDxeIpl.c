/** @file
  Validate, load and enter the pinned BridgeVM PC DXE Core image.

  SPDX-License-Identifier: Apache-2.0
**/

#include "BridgeVmPcDxeIpl.h"

static void
SetHeader(volatile BRIDGE_VM_PC_HOB_HEADER *Header, uint16_t Type, uint16_t Length)
{
  Header->HobType = Type;
  Header->HobLength = Length;
  Header->Reserved = 0;
}

static void
SetGuid(
  volatile BRIDGE_VM_PC_GUID *Guid,
  uint32_t Data1,
  uint16_t Data2,
  uint16_t Data3,
  const uint8_t Data4[8]
  )
{
  uint32_t Index;

  Guid->Data1 = Data1;
  Guid->Data2 = Data2;
  Guid->Data3 = Data3;
  for (Index = 0; Index < 8; ++Index) {
    Guid->Data4[Index] = Data4[Index];
  }
}

static uint32_t
FirmwareVolumeIsValid(const volatile uint8_t *Fv)
{
  const volatile uint16_t *HeaderWords = (const volatile uint16_t *)Fv;
  const volatile uint32_t *CoreGuid;
  uint32_t Index;
  uint32_t Sum = 0;

  if (*(const volatile uint64_t *)(Fv + 0x20) != BRIDGE_VM_PC_DXE_FV_SIZE ||
      *(const volatile uint32_t *)(Fv + 0x28) != 0x4856465FU ||
      *(const volatile uint16_t *)(Fv + 0x30) != 0x48U) {
    return 0;
  }
  for (Index = 0; Index < 0x48U / sizeof(uint16_t); ++Index) {
    Sum += HeaderWords[Index];
  }
  if ((uint16_t)Sum != 0) {
    return 0;
  }
  CoreGuid = (const volatile uint32_t *)(Fv + BRIDGE_VM_PC_DXE_CORE_FFS_OFFSET);
  return CoreGuid[0] == 0xD6A2CB7FU && CoreGuid[1] == 0x4E2F6A18U &&
         CoreGuid[2] == 0x20993BB4U && CoreGuid[3] == 0x0A7033A7U;
}

static uint32_t
DxeCoreImageIsValid(const volatile uint8_t *Image)
{
  uint32_t PeOffset;
  const volatile uint8_t *Optional;

  if (*(const volatile uint16_t *)Image != 0x5A4DU) {
    return 0;
  }
  PeOffset = *(const volatile uint32_t *)(Image + 0x3C);
  if (PeOffset != 0xE58U ||
      *(const volatile uint32_t *)(Image + PeOffset) != 0x00004550U ||
      *(const volatile uint16_t *)(Image + PeOffset + 4) != 0xAA64U) {
    return 0;
  }
  Optional = Image + PeOffset + 24;
  return *(const volatile uint16_t *)Optional == 0x20BU &&
         *(const volatile uint32_t *)(Optional + 16) == 0x6BF4U &&
         *(const volatile uint32_t *)(Optional + 24) == 0x00400000U &&
         *(const volatile uint32_t *)(Optional + 28) == 0x00000001U &&
         *(const volatile uint32_t *)(Optional + 56) == BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE &&
         *(const volatile uint32_t *)(Optional + 60) == 0x1000U;
}

static void
AppendDxeHobs(volatile BRIDGE_VM_PC_SEC_RESULT *Result, volatile uint8_t *HobList)
{
  static const uint8_t ModuleAllocationData4[8] =
    {0xA4, 0xBE, 0x55, 0x25, 0xA9, 0xC6, 0xD7, 0x7A};
  static const uint8_t DxeCoreData4[8] =
    {0xB4, 0x3B, 0x99, 0x20, 0xA7, 0x33, 0x70, 0x0A};
  volatile BRIDGE_VM_PC_HOB_HANDOFF_INFO *Phit =
    (volatile BRIDGE_VM_PC_HOB_HANDOFF_INFO *)HobList;
  volatile BRIDGE_VM_PC_HOB_FV_INFO *Fv =
    (volatile BRIDGE_VM_PC_HOB_FV_INFO *)(HobList + 216);
  volatile BRIDGE_VM_PC_HOB_MODULE *Module =
    (volatile BRIDGE_VM_PC_HOB_MODULE *)(HobList + 240);
  volatile BRIDGE_VM_PC_HOB_HEADER *End =
    (volatile BRIDGE_VM_PC_HOB_HEADER *)(HobList + 312);
  uint32_t Index;

  SetHeader(&Fv->Header, BRIDGE_VM_PC_HOB_FV, sizeof(*Fv));
  Fv->BaseAddress = BRIDGE_VM_PC_DXE_FV_GPA;
  Fv->Length = BRIDGE_VM_PC_DXE_FV_SIZE;
  SetHeader(&Module->Header, BRIDGE_VM_PC_HOB_MEMORY_ALLOCATION, sizeof(*Module));
  SetGuid(&Module->Allocation.Name, 0xF8E21975U, 0x0899U, 0x4F58U,
          ModuleAllocationData4);
  Module->Allocation.MemoryBaseAddress = BRIDGE_VM_PC_DXE_CORE_LOAD_BASE;
  Module->Allocation.MemoryLength = BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE;
  Module->Allocation.MemoryType = BRIDGE_VM_PC_EFI_BOOT_SERVICES_CODE;
  for (Index = 0; Index < 4; ++Index) {
    Module->Allocation.Reserved[Index] = 0;
  }
  SetGuid(&Module->ModuleName, 0xD6A2CB7FU, 0x6A18U, 0x4E2FU, DxeCoreData4);
  Module->EntryPoint = BRIDGE_VM_PC_DXE_CORE_ENTRY;
  SetHeader(End, BRIDGE_VM_PC_HOB_END, sizeof(*End));
  Phit->EfiFreeMemoryBottom = BRIDGE_VM_PC_DXE_CORE_LOAD_BASE + BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE;
  Phit->EfiEndOfHobList = BRIDGE_VM_PC_HOB_LIST_GPA + 312;
  Result->HobCount = BRIDGE_VM_PC_DXE_HOB_COUNT;
  Result->HobListSize = BRIDGE_VM_PC_DXE_HOB_LIST_SIZE;
}

static void
CleanDataToPointOfCoherency(const volatile void *Base, uint32_t Size)
{
  uintptr_t Address;
  uintptr_t End = (uintptr_t)Base + Size;
  uint64_t Ctr;
  uintptr_t LineSize;
  __asm__ __volatile__("mrs %0, ctr_el0" : "=r" (Ctr));
  LineSize = 4U << ((Ctr >> 16) & 0xFU);
  Address = (uintptr_t)Base & ~(LineSize - 1U);
  for (; Address < End; Address += LineSize) {
    __asm__ __volatile__("dc cvac, %0" :: "r" (Address) : "memory");
  }
  __asm__ __volatile__("dsb sy" ::: "memory");
}

static uint32_t
ReturnStage(volatile BRIDGE_VM_PC_SEC_RESULT *Result, uint32_t Stage)
{
  Result->Stage = Stage;
  CleanDataToPointOfCoherency(Result, sizeof(*Result));
  return Stage;
}

static void
CopyAndSynchronizeDxeCore(const volatile uint8_t *Image)
{
  const volatile uint32_t *Source = (const volatile uint32_t *)Image;
  volatile uint32_t *Destination =
    (volatile uint32_t *)(uintptr_t)BRIDGE_VM_PC_DXE_CORE_LOAD_BASE;
  uintptr_t Address;
  uint64_t Ctr;
  uintptr_t DataLine;
  uintptr_t InstructionLine;
  uint32_t Index;

  for (Index = 0; Index < BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE / sizeof(uint32_t); ++Index) {
    Destination[Index] = Source[Index];
  }
  __asm__ __volatile__("mrs %0, ctr_el0" : "=r" (Ctr));
  DataLine = 4U << ((Ctr >> 16) & 0xFU);
  InstructionLine = 4U << (Ctr & 0xFU);
  for (Address = BRIDGE_VM_PC_DXE_CORE_LOAD_BASE;
       Address < BRIDGE_VM_PC_DXE_CORE_LOAD_BASE + BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE;
       Address += DataLine) {
    __asm__ __volatile__("dc cvau, %0" :: "r" (Address) : "memory");
  }
  __asm__ __volatile__("dsb ish" ::: "memory");
  for (Address = BRIDGE_VM_PC_DXE_CORE_LOAD_BASE;
       Address < BRIDGE_VM_PC_DXE_CORE_LOAD_BASE + BRIDGE_VM_PC_DXE_CORE_IMAGE_SIZE;
       Address += InstructionLine) {
    __asm__ __volatile__("ic ivau, %0" :: "r" (Address) : "memory");
  }
  __asm__ __volatile__("dsb ish\n\tisb" ::: "memory");
}

uint32_t
BridgeVmPcDxeIplMain(
  volatile BRIDGE_VM_PC_SEC_RESULT *Result,
  volatile void *HobList
  )
{
  const volatile uint8_t *Fv =
    (const volatile uint8_t *)(uintptr_t)BRIDGE_VM_PC_DXE_FV_GPA;
  const volatile uint8_t *Image = Fv + BRIDGE_VM_PC_DXE_CORE_PE_OFFSET;
  void (*DxeEntry)(void *);

  if (Result == (void *)0) {
    return BRIDGE_VM_PC_SEC_BAD_HOB;
  }
  if (HobList == (void *)0 || Result->Stage != BRIDGE_VM_PC_SEC_SUCCESS) {
    return ReturnStage(Result, BRIDGE_VM_PC_SEC_BAD_HOB);
  }
  if (!FirmwareVolumeIsValid(Fv)) {
    return ReturnStage(Result, BRIDGE_VM_PC_SEC_BAD_FV);
  }
  if (!DxeCoreImageIsValid(Image)) {
    return ReturnStage(Result, BRIDGE_VM_PC_SEC_BAD_DXE_IMAGE);
  }
  AppendDxeHobs(Result, (volatile uint8_t *)HobList);
  CopyAndSynchronizeDxeCore(Image);
  DxeEntry = (void (*)(void *))(uintptr_t)BRIDGE_VM_PC_DXE_CORE_ENTRY;
  DxeEntry((void *)HobList);
  CleanDataToPointOfCoherency(HobList, BRIDGE_VM_PC_DXE_HOB_LIST_SIZE);
  return ReturnStage(Result, BRIDGE_VM_PC_SEC_DXE_RETURNED);
}
