/** @file
  Private variable-service proof interface.

  SPDX-License-Identifier: Apache-2.0
**/

#ifndef BRIDGE_VM_PC_VARIABLE_PROBE_H_
#define BRIDGE_VM_PC_VARIABLE_PROBE_H_

#include <Uefi.h>

typedef struct {
  UINT32 State;
  UINT32 Attributes;
  UINT64 GetVariable;
  UINT64 SetVariable;
  UINT64 QueryVariableInfo;
  UINT64 MaximumStorage;
  UINT64 RemainingStorage;
  UINT64 MaximumVariableSize;
} BRIDGE_VM_PC_VARIABLE_RESULT;

EFI_STATUS
BridgeVmPcRunVariableProbe (
  IN EFI_SYSTEM_TABLE                *SystemTable,
  OUT BRIDGE_VM_PC_VARIABLE_RESULT  *Result
  );

#endif
