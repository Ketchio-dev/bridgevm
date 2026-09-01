/** @file
  Register the emulated variable store for UEFI virtual-address conversion.

  SPDX-License-Identifier: Apache-2.0
**/

#include <PiDxe.h>
#include <Guid/DxeServices.h>
#include <Library/BaseMemoryLib.h>
#include <Library/DebugLib.h>

#include "PlatformTablesDxe.h"

#define BRIDGE_VM_PC_VARIABLE_STORE_BASE  0x04000000ULL
#define BRIDGE_VM_PC_VARIABLE_STORE_SIZE  0x00010000ULL

EFI_STATUS
BridgeVmPcRegisterVariableStoreRuntimeMemory (
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  EFI_DXE_SERVICES  *DxeServices;
  EFI_STATUS        Status;
  UINTN             Index;

  DxeServices = NULL;
  for (Index = 0; Index < SystemTable->NumberOfTableEntries; Index++) {
    if (CompareGuid (&SystemTable->ConfigurationTable[Index].VendorGuid, &gEfiDxeServicesTableGuid)) {
      DxeServices = SystemTable->ConfigurationTable[Index].VendorTable;
      break;
    }
  }

  if (DxeServices == NULL) {
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: DXE services table is missing\n"));
    return EFI_NOT_FOUND;
  }

  Status = DxeServices->AddMemorySpace (
                  EfiGcdMemoryTypeMemoryMappedIo,
                  BRIDGE_VM_PC_VARIABLE_STORE_BASE,
                  BRIDGE_VM_PC_VARIABLE_STORE_SIZE,
                  EFI_MEMORY_UC | EFI_MEMORY_RUNTIME | EFI_MEMORY_XP
                  );
  if (EFI_ERROR (Status)) {
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: variable runtime MMIO registration failed: %r\n", Status));
    return Status;
  }

  Status = DxeServices->SetMemorySpaceAttributes (
                  BRIDGE_VM_PC_VARIABLE_STORE_BASE,
                  BRIDGE_VM_PC_VARIABLE_STORE_SIZE,
                  EFI_MEMORY_UC | EFI_MEMORY_RUNTIME | EFI_MEMORY_XP
                  );
  if (EFI_ERROR (Status)) {
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: variable runtime MMIO attributes failed: %r\n", Status));
  }

  return Status;
}
