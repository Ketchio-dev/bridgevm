/** @file
  Removable-media application proving the UEFI ExitBootServices transition.
  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Library/BaseLib.h>
#include <BridgeVmPc/BootResult.h>

STATIC volatile BRIDGE_VM_PC_BOOT_RESULT *
Result (VOID)
{
  return (volatile BRIDGE_VM_PC_BOOT_RESULT *)(UINTN)BRIDGE_VM_PC_BOOT_RESULT_GPA;
}

STATIC VOID
Stop (IN UINT32 Stage, IN EFI_STATUS Status)
{
  Result ()->Status = Status;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_ERROR | Stage;
  __asm__ __volatile__("dsb sy\n\thvc #1" ::: "memory");
  CpuDeadLoop ();
}

EFI_STATUS EFIAPI
UefiMain (IN EFI_HANDLE ImageHandle, IN EFI_SYSTEM_TABLE *SystemTable)
{
  EFI_BOOT_SERVICES *BootServices;
  EFI_MEMORY_DESCRIPTOR *Map;
  EFI_STATUS Status;
  UINTN MapSize;
  UINTN MapKey;
  UINTN DescriptorSize;
  UINT32 DescriptorVersion;
  UINTN Attempt;
  if ((SystemTable == NULL) || (SystemTable->BootServices == NULL)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_APPLICATION_ENTRY, EFI_INVALID_PARAMETER);
  }
  BootServices = SystemTable->BootServices;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_APPLICATION_ENTRY;
  Map = NULL;
  MapSize = 0;
  DescriptorSize = 0;
  Status = BootServices->GetMemoryMap (&MapSize, Map, &MapKey,
                                       &DescriptorSize, &DescriptorVersion);
  if ((Status != EFI_BUFFER_TOO_SMALL) || (DescriptorSize == 0)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_MEMORY_MAP_READY, Status);
  }
  MapSize += 8 * DescriptorSize;
  Status = BootServices->AllocatePool (EfiLoaderData, MapSize, (VOID **)&Map);
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_MEMORY_MAP_READY, Status);
  }
  for (Attempt = 1; Attempt <= 3; ++Attempt) {
    UINTN Capacity = MapSize;
    Status = BootServices->GetMemoryMap (&Capacity, Map, &MapKey,
                                         &DescriptorSize, &DescriptorVersion);
    if (EFI_ERROR (Status)) {
      Stop (BRIDGE_VM_PC_BOOT_STAGE_MEMORY_MAP_READY, Status);
    }
    Result ()->MemoryMapSize = Capacity;
    Result ()->MapKey = MapKey;
    Result ()->DescriptorSize = DescriptorSize;
    Result ()->DescriptorVersion = DescriptorVersion;
    Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_MEMORY_MAP_READY;
    Result ()->ExitBootServicesAttempts = (UINT32)Attempt;
    Status = BootServices->ExitBootServices (ImageHandle, MapKey);
    if (!EFI_ERROR (Status)) {
      break;
    }
    if (Status != EFI_INVALID_PARAMETER) {
      Stop (BRIDGE_VM_PC_BOOT_STAGE_EXIT_BOOT_SERVICES, Status);
    }
  }
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_EXIT_BOOT_SERVICES, Status);
  }
  Result ()->Status = EFI_SUCCESS;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_EXIT_BOOT_SERVICES;
  __asm__ __volatile__("dsb sy" ::: "memory");
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_POST_EXIT;
  __asm__ __volatile__("dsb sy\n\thvc #0" ::: "memory");
  CpuDeadLoop ();
  return EFI_DEVICE_ERROR;
}
