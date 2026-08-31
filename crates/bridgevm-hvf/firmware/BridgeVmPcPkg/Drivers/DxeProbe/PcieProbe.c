// BridgeVM Virtual ARM PC PCIe probe. SPDX-License-Identifier: Apache-2.0
#include "PcieProbe.h"

#define BRIDGE_VM_PC_PCIE_ECAM_BASE  0x40000000ULL

STATIC CONST UINT32  mExpectedPcieIdentity[BRIDGE_VM_PC_PCIE_FUNCTION_COUNT] = {
  0x00081B36U,
  0x00101B36U,
  0x000D1B36U,
  0x10011AF4U,
  0x10411AF4U,
  0x10501AF4U,
  0x10431AF4U,
  0x26688086U
};

EFI_STATUS
BridgeVmPcValidatePcieIdentities (
  OUT volatile UINT32  *Observed
  )
{
  UINTN   Index;
  UINT64  Address;

  for (Index = 0; Index < BRIDGE_VM_PC_PCIE_FUNCTION_COUNT; ++Index) {
    Address = BRIDGE_VM_PC_PCIE_ECAM_BASE + (Index << 15);
    Observed[Index] = *(volatile UINT32 *)(UINTN)Address;
    if (Observed[Index] != mExpectedPcieIdentity[Index]) {
      return EFI_COMPROMISED_DATA;
    }
  }

  return EFI_SUCCESS;
}
