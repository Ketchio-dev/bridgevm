// SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGE_VM_PC_PCI_PROBE_INTERNAL_H_
#define BRIDGE_VM_PC_PCI_PROBE_INTERNAL_H_
#include <Library/BridgeVmPcPciProbeLib.h>
EFI_STATUS BridgeVmPcDiscoverPci (IN EFI_SYSTEM_TABLE *SystemTable, OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result);
EFI_STATUS BridgeVmPcValidatePciIdentities (IN EFI_SYSTEM_TABLE *SystemTable, OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result);
EFI_STATUS BridgeVmPcValidateNvmeBar (IN EFI_SYSTEM_TABLE *SystemTable, OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result);
#endif
