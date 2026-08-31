// SPDX-License-Identifier: Apache-2.0
#include "PciProbeInternal.h"
#include <Protocol/DriverBinding.h>
#include <Protocol/PciEnumerationComplete.h>
#include <Protocol/PciRootBridgeIo.h>

EFI_STATUS
BridgeVmPcDiscoverPci (
  IN EFI_SYSTEM_TABLE *SystemTable,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_BOOT_SERVICES *BootServices;
  EFI_DRIVER_BINDING_PROTOCOL *DriverBinding;
  EFI_HANDLE *Handles;
  EFI_HANDLE RootBridge;
  EFI_STATUS Status;
  UINTN Count;
  UINTN Index;
  VOID *Complete;

  BootServices = SystemTable->BootServices;
  Handles = NULL;
  Count = 0;
  Status = BootServices->LocateHandleBuffer (
                           ByProtocol,
                           &gEfiPciRootBridgeIoProtocolGuid,
                           NULL,
                           &Count,
                           &Handles
                           );
  if (EFI_ERROR (Status) || (Count != 1)) {
    return EFI_NOT_FOUND;
  }
  Result->RootBridgeCount = (UINT32)Count;
  RootBridge = Handles[0];
  BootServices->FreePool (Handles);
  Handles = NULL;
  Count = 0;
  Status = BootServices->LocateHandleBuffer (
                           ByProtocol,
                           &gEfiDriverBindingProtocolGuid,
                           NULL,
                           &Count,
                           &Handles
                           );
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Result->DriverBindingCount = (UINT32)Count;
  Result->SupportedStatus = (UINT32)EFI_NOT_FOUND;
  for (Index = 0; Index < Count; ++Index) {
    DriverBinding = NULL;
    Status = BootServices->HandleProtocol (
                             Handles[Index],
                             &gEfiDriverBindingProtocolGuid,
                             (VOID **)&DriverBinding
                             );
    if (!EFI_ERROR (Status) && (DriverBinding != NULL)) {
      Status = DriverBinding->Supported (DriverBinding, RootBridge, NULL);
      Result->SupportedStatus = (UINT32)Status;
      if (!EFI_ERROR (Status)) {
        break;
      }
    }
  }
  BootServices->FreePool (Handles);
  Status = BootServices->ConnectController (RootBridge, NULL, NULL, TRUE);
  Result->ConnectStatus = (UINT32)Status;
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Complete = NULL;
  Status = BootServices->LocateProtocol (
                           &gEfiPciEnumerationCompleteProtocolGuid,
                           NULL,
                           &Complete
                           );
  if (EFI_ERROR (Status)) {
    return EFI_NOT_FOUND;
  }
  Result->EnumerationComplete = 1;
  return EFI_SUCCESS;
}
