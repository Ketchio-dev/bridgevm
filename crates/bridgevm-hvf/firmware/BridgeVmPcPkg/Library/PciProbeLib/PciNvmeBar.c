// SPDX-License-Identifier: Apache-2.0
#include "PciProbeInternal.h"
#include <IndustryStandard/Acpi10.h>
#include <IndustryStandard/Pci22.h>
#include <Protocol/PciIo.h>

#define BRIDGE_VM_PC_NVME_DEVICE       1U
#define BRIDGE_VM_PC_NVME_BAR_LENGTH   0x4000ULL
#define BRIDGE_VM_PC_NVME_CAP          0x00000020020103FFULL
#define BRIDGE_VM_PC_NVME_VERSION      0x00010400U
#define BRIDGE_VM_PC_NVME_COMMAND_MASK (EFI_PCI_COMMAND_MEMORY_SPACE | EFI_PCI_COMMAND_BUS_MASTER)
STATIC_ASSERT (sizeof (BRIDGE_VM_PC_PCIE_RESULT) == 96, "unexpected PCI result size");
STATIC
BOOLEAN
BridgeVmPcNvmeBarBaseIsValid (
  IN UINT64 Base
  )
{
  if ((Base & (BRIDGE_VM_PC_NVME_BAR_LENGTH - 1)) != 0) {
    return FALSE;
  }
  return ((Base >= 0x50000000ULL) &&
          (Base <= 0x100000000ULL - BRIDGE_VM_PC_NVME_BAR_LENGTH)) ||
         ((Base >= 0x2000000000ULL) &&
          (Base <= 0x3000000000ULL - BRIDGE_VM_PC_NVME_BAR_LENGTH));
}

STATIC
EFI_STATUS
BridgeVmPcReadNvmeBar (
  IN EFI_BOOT_SERVICES *BootServices,
  IN EFI_PCI_IO_PROTOCOL *PciIo,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_ACPI_ADDRESS_SPACE_DESCRIPTOR *Descriptor;
  EFI_STATUS Status;
  VOID *Resources;
  UINT16 Command;
  UINT32 Version;
  UINT64 Capabilities;

  Resources = NULL;
  Status = PciIo->GetBarAttributes (PciIo, 0, NULL, &Resources);
  if (EFI_ERROR (Status) || (Resources == NULL)) {
    return EFI_NOT_FOUND;
  }
  Descriptor = (EFI_ACPI_ADDRESS_SPACE_DESCRIPTOR *)Resources;
  Result->NvmeBarResourceType = Descriptor->ResType;
  Result->NvmeBarBase = Descriptor->AddrRangeMin;
  Result->NvmeBarLength = Descriptor->AddrLen;
  if ((Descriptor->Desc != ACPI_QWORD_ADDRESS_SPACE_DESCRIPTOR) ||
      (Descriptor->ResType != ACPI_ADDRESS_SPACE_TYPE_MEM) ||
      (Descriptor->AddrSpaceGranularity != 64) ||
      (Descriptor->AddrLen != BRIDGE_VM_PC_NVME_BAR_LENGTH) ||
      !BridgeVmPcNvmeBarBaseIsValid (Descriptor->AddrRangeMin))
  {
    BootServices->FreePool (Resources);
    return EFI_COMPROMISED_DATA;
  }
  BootServices->FreePool (Resources);
  Status = PciIo->Attributes (
                    PciIo,
                    EfiPciIoAttributeOperationEnable,
                    EFI_PCI_IO_ATTRIBUTE_MEMORY | EFI_PCI_IO_ATTRIBUTE_BUS_MASTER,
                    NULL
                    );
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Command = 0;
  Status = PciIo->Pci.Read (PciIo, EfiPciIoWidthUint16, 4, 1, &Command);
  if (EFI_ERROR (Status) || ((Command & BRIDGE_VM_PC_NVME_COMMAND_MASK) != BRIDGE_VM_PC_NVME_COMMAND_MASK)) {
    return EFI_COMPROMISED_DATA;
  }
  Capabilities = 0;
  Status = PciIo->Mem.Read (PciIo, EfiPciIoWidthUint64, 0, 0, 1, &Capabilities);
  if (EFI_ERROR (Status) || (Capabilities != BRIDGE_VM_PC_NVME_CAP)) {
    return EFI_COMPROMISED_DATA;
  }
  Version = 0;
  Status = PciIo->Mem.Read (PciIo, EfiPciIoWidthUint32, 0, 8, 1, &Version);
  if (EFI_ERROR (Status) || (Version != BRIDGE_VM_PC_NVME_VERSION)) {
    return EFI_COMPROMISED_DATA;
  }
  Result->NvmeCommand = Command;
  Result->NvmeControllerCapabilities = Capabilities;
  Result->NvmeVersion = Version;
  Result->NvmeBarReadCount = 2;
  return EFI_SUCCESS;
}

EFI_STATUS
BridgeVmPcValidateNvmeBar (
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
    return Status;
  }
  Status = EFI_NOT_FOUND;
  for (Index = 0; Index < Count; ++Index) {
    PciIo = NULL;
    if (EFI_ERROR (BootServices->HandleProtocol (
                                      Handles[Index],
                                      &gEfiPciIoProtocolGuid,
                                      (VOID **)&PciIo
                                      )) || (PciIo == NULL))
    {
      continue;
    }
    if (!EFI_ERROR (PciIo->GetLocation (PciIo, &Segment, &Bus, &Device, &Function)) &&
        (Segment == 0) && (Bus == 0) && (Device == BRIDGE_VM_PC_NVME_DEVICE) && (Function == 0))
    {
      Status = BridgeVmPcReadNvmeBar (BootServices, PciIo, Result);
      break;
    }
  }
  BootServices->FreePool (Handles);
  return Status;
}

EFI_STATUS
BridgeVmPcValidatePcie (
  IN EFI_SYSTEM_TABLE *SystemTable,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_STATUS Status;

  if ((SystemTable == NULL) || (SystemTable->BootServices == NULL) || (Result == NULL)) {
    return EFI_INVALID_PARAMETER;
  }
  Result->FunctionCount = BRIDGE_VM_PC_PCIE_FUNCTION_COUNT;
  Status = BridgeVmPcDiscoverPci (SystemTable, Result);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Status = BridgeVmPcValidatePciIdentities (SystemTable, Result);
  return EFI_ERROR (Status) ? Status : BridgeVmPcValidateNvmeBar (SystemTable, Result);
}
