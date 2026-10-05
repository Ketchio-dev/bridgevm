// SPDX-License-Identifier: Apache-2.0
#include "PciProbeInternal.h"
#include <Protocol/PciIo.h>

#include "PciIdentityContract.h"

EFI_STATUS
BridgeVmPcValidatePciIdentities (
  IN EFI_SYSTEM_TABLE *SystemTable,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_BOOT_SERVICES *BootServices;
  EFI_HANDLE *Handles;
  EFI_PCI_IO_PROTOCOL *PciIo;
  EFI_STATUS Status;
  UINTN Count;
  UINTN Index;
  UINTN Segment;
  UINTN Bus;
  UINTN Device;
  UINTN Function;
  UINT32 Identity;
  UINT32 Seen;

  BootServices = SystemTable->BootServices;
  Handles = NULL;
  Count = 0;
  Status = BootServices->LocateHandleBuffer (
                           ByProtocol,
                           &gEfiPciIoProtocolGuid,
                           NULL,
                           &Count,
                           &Handles
                           );
  if (EFI_ERROR (Status)) {
    return EFI_COMPROMISED_DATA;
  }
  if (Count != BRIDGE_VM_PC_PCIE_FUNCTION_COUNT) {
    BootServices->FreePool (Handles);
    return EFI_COMPROMISED_DATA;
  }
  BridgeVmPcClearIdentitySlots (Result);
  Seen = 0;
  for (Index = 0; Index < Count; ++Index) {
    PciIo = NULL;
    Status = BootServices->HandleProtocol (
                             Handles[Index],
                             &gEfiPciIoProtocolGuid,
                             (VOID **)&PciIo
                             );
    if (EFI_ERROR (Status) || (PciIo == NULL)) {
      break;
    }
    Status = PciIo->GetLocation (PciIo, &Segment, &Bus, &Device, &Function);
    if (EFI_ERROR (Status) || !BridgeVmPcIdentityLocationValid (Segment, Bus, Device, Function, Seen)) {
      break;
    }
    Identity = 0;
    Status = PciIo->Pci.Read (PciIo, EfiPciIoWidthUint32, 0, 1, &Identity);
    if (EFI_ERROR (Status) || (Identity != mExpectedIdentity[Device])) {
      break;
    }
    Result->Identity[Device] = Identity;
    Seen |= 1U << Device;
  }
  BootServices->FreePool (Handles);
  return (Seen == ((1U << BRIDGE_VM_PC_PCIE_FUNCTION_COUNT) - 1U)) ? EFI_SUCCESS : EFI_COMPROMISED_DATA;
}
