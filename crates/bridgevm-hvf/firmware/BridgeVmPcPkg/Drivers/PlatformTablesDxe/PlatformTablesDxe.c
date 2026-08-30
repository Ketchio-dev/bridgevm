/** @file
  Publish BridgeVM-owned ACPI and SMBIOS tables through standard UEFI GUIDs.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include <Guid/Acpi.h>
#include <Guid/SmBios.h>
#include <Library/DebugLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiDriverEntryPoint.h>
#include <BridgeVmPc/BootInfo.h>

EFI_STATUS
EFIAPI
BridgeVmPcPlatformTablesDxeEntryPoint (
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  CONST BRIDGE_VM_PC_BOOT_INFO  *BootInfo;
  EFI_STATUS                    Status;

  (VOID)ImageHandle;
  (VOID)SystemTable;

  Status = BridgeVmPcValidateBootInfo (&BootInfo);
  if (EFI_ERROR (Status)) {
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: boot-info validation failed: %r\n", Status));
    return Status;
  }

  Status = gBS->InstallConfigurationTable (
                  &gEfiAcpi20TableGuid,
                  (VOID *)(UINTN)BootInfo->RsdpGpa
                  );
  if (EFI_ERROR (Status)) {
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: ACPI publication failed: %r\n", Status));
    return Status;
  }

  Status = gBS->InstallConfigurationTable (
                  &gEfiSmbios3TableGuid,
                  (VOID *)(UINTN)BootInfo->SmbiosAnchorGpa
                  );
  if (EFI_ERROR (Status)) {
    gBS->InstallConfigurationTable (&gEfiAcpi20TableGuid, NULL);
    DEBUG ((DEBUG_ERROR, "BridgeVM PC: SMBIOS publication failed: %r\n", Status));
    return Status;
  }

  DEBUG ((DEBUG_INFO, "BridgeVM PC: ACPI and SMBIOS configuration tables published\n"));
  return EFI_SUCCESS;
}
