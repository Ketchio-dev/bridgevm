// BridgeVM Virtual ARM PC PCI probe contract. SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGE_VM_PC_PCIE_PROBE_LIB_H_
#define BRIDGE_VM_PC_PCIE_PROBE_LIB_H_
#include <Uefi.h>
#define BRIDGE_VM_PC_PCIE_FUNCTION_COUNT  8U
typedef struct {
  UINT32 FunctionCount, Identity[BRIDGE_VM_PC_PCIE_FUNCTION_COUNT];
  UINT32 RootBridgeCount, EnumerationComplete, DriverBindingCount, SupportedStatus, ConnectStatus;
  UINT32 NvmeBarReadCount, NvmeBarResourceType;
  UINT64 NvmeBarBase, NvmeBarLength, NvmeControllerCapabilities;
  UINT32 NvmeVersion, NvmeCommand;
} BRIDGE_VM_PC_PCIE_RESULT;
EFI_STATUS BridgeVmPcValidatePcie (
  IN EFI_SYSTEM_TABLE *SystemTable, OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  );
#endif
