/** @file
  UEFI PCI root apertures for the BridgeVM Virtual ARM PC.

  SPDX-License-Identifier: Apache-2.0
**/

#include <PiDxe.h>
#include <IndustryStandard/Pci.h>
#include <Protocol/PciHostBridgeResourceAllocation.h>
#include <Library/BaseMemoryLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/PciHostBridgeLib.h>

typedef struct {
  ACPI_HID_DEVICE_PATH       Acpi;
  EFI_DEVICE_PATH_PROTOCOL  End;
} BRIDGE_VM_PC_ROOT_DEVICE_PATH;

STATIC CONST BRIDGE_VM_PC_ROOT_DEVICE_PATH  mRootDevicePath = {
  {
    { ACPI_DEVICE_PATH, ACPI_DP, { sizeof (ACPI_HID_DEVICE_PATH), 0 } },
    EISA_PNP_ID (0x0A08),
    0
  },
  { END_DEVICE_PATH_TYPE, END_ENTIRE_DEVICE_PATH_SUBTYPE, { sizeof (EFI_DEVICE_PATH_PROTOCOL), 0 } }
};

PCI_ROOT_BRIDGE *
EFIAPI
PciHostBridgeGetRootBridges (
  UINTN  *Count
  )
{
  PCI_ROOT_BRIDGE  *Bridge;

  if (Count == NULL) {
    return NULL;
  }
  Bridge = AllocateZeroPool (sizeof (*Bridge));
  if (Bridge == NULL) {
    *Count = 0;
    return NULL;
  }
  Bridge->Segment = 0;
  Bridge->DmaAbove4G = TRUE;
  Bridge->NoExtendedConfigSpace = FALSE;
  Bridge->ResourceAssigned = FALSE;
  Bridge->AllocationAttributes = EFI_PCI_HOST_BRIDGE_MEM64_DECODE;
  Bridge->Bus = (PCI_ROOT_BRIDGE_APERTURE){ 0, 0xFF, 0 };
  Bridge->Io = (PCI_ROOT_BRIDGE_APERTURE){ MAX_UINT64, 0, 0 };
  Bridge->Mem = (PCI_ROOT_BRIDGE_APERTURE){ 0x50000000ULL, 0xFFFFFFFFULL, 0 };
  Bridge->MemAbove4G = (PCI_ROOT_BRIDGE_APERTURE){ 0x2000000000ULL, 0x2FFFFFFFFFULL, 0 };
  Bridge->PMem = (PCI_ROOT_BRIDGE_APERTURE){ MAX_UINT64, 0, 0 };
  Bridge->PMemAbove4G = (PCI_ROOT_BRIDGE_APERTURE){ 0x3000000000ULL, 0x3FFFFFFFFFULL, 0 };
  Bridge->DevicePath = AllocateCopyPool (sizeof (mRootDevicePath), &mRootDevicePath);
  if (Bridge->DevicePath == NULL) {
    FreePool (Bridge);
    *Count = 0;
    return NULL;
  }
  *Count = 1;
  return Bridge;
}

VOID
EFIAPI
PciHostBridgeFreeRootBridges (
  PCI_ROOT_BRIDGE  *Bridges,
  UINTN            Count
  )
{
  if ((Bridges != NULL) && (Count == 1)) {
    FreePool (Bridges->DevicePath);
    FreePool (Bridges);
  }
}

VOID
EFIAPI
PciHostBridgeResourceConflict (
  EFI_HANDLE  HostBridgeHandle,
  VOID        *Configuration
  )
{
  (VOID)HostBridgeHandle;
  (VOID)Configuration;
}
