// SPDX-License-Identifier: Apache-2.0
#include "PciProbeInternal.h"

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
  return BridgeVmPcValidatePciIdentities (SystemTable, Result);
}
