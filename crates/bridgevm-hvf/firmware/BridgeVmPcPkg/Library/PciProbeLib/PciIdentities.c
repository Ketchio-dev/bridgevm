// SPDX-License-Identifier: Apache-2.0
#include "PciProbeInternal.h"
#include <Protocol/PciIo.h>

STATIC CONST UINT32 mExpectedIdentity[BRIDGE_VM_PC_PCIE_FUNCTION_COUNT] = {
  0x00081B36U, 0x00101B36U, 0x000D1B36U, 0x10011AF4U,
  0x10411AF4U, 0x10501AF4U, 0x10431AF4U, 0x26688086U
};

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
  if (EFI_ERROR (Status) || (Count != BRIDGE_VM_PC_PCIE_FUNCTION_COUNT)) {
    return EFI_COMPROMISED_DATA;
  }
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
    if (EFI_ERROR (Status) || (Segment != 0) || (Bus != 0) ||
        (Device >= Count) || (Function != 0) || ((Seen & (1U << Device)) != 0))
    {
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
  return (Seen == 0xFFU) ? EFI_SUCCESS : EFI_COMPROMISED_DATA;
}
