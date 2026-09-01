// BridgeVM Virtual ARM PC DXE result. SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGE_VM_PC_DXE_RESULT_H_
#define BRIDGE_VM_PC_DXE_RESULT_H_

#include <Uefi.h>
#include <Library/BridgeVmPcPciProbeLib.h>
#include "VariableProbe.h"
#define BRIDGE_VM_PC_DXE_RESULT_GPA  0x100002000ULL

typedef struct {
  UINT32 Stage;
  UINT32 RuntimeCrc32;
  UINT64 SystemTable;
  UINT64 RuntimeServices;
  UINT64 RuntimeProtocol;
  UINT64 SetVirtualAddressMap;
  UINT64 ConvertPointer;
  UINT64 CalculateCrc32;
  BRIDGE_VM_PC_VARIABLE_RESULT Variable;
  BRIDGE_VM_PC_PCIE_RESULT Pcie;
} BRIDGE_VM_PC_DXE_RESULT;

#endif
