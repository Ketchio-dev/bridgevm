/** @file
  Private interface for the bounded RuntimeDxe live probe.

  SPDX-License-Identifier: Apache-2.0
**/

#ifndef BRIDGE_VM_PC_RUNTIME_PROBE_H_
#define BRIDGE_VM_PC_RUNTIME_PROBE_H_

#include <Uefi.h>

EFI_STATUS
BridgeVmPcRunRuntimeProbe (
  IN EFI_SYSTEM_TABLE  *SystemTable
  );

#endif
