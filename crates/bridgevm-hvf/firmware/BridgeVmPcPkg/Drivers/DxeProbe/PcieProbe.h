// BridgeVM Virtual ARM PC PCIe probe. SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGE_VM_PC_PCIE_PROBE_H_
#define BRIDGE_VM_PC_PCIE_PROBE_H_

#include <Uefi.h>

#define BRIDGE_VM_PC_PCIE_FUNCTION_COUNT  8U

EFI_STATUS
BridgeVmPcValidatePcieIdentities (
  OUT volatile UINT32  *Observed
  );

#endif
