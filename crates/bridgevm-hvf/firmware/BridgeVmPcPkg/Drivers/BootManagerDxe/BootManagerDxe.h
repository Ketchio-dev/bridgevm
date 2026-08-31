/** @file
  Internal contract between the BridgeVM BDS driver's translation units.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_BOOT_MANAGER_DXE_H_
#define BRIDGE_VM_PC_BOOT_MANAGER_DXE_H_

#include <Uefi.h>
#include <BridgeVmPc/BootResult.h>

#define BRIDGE_VM_PC_RESULT() \
  ((volatile BRIDGE_VM_PC_BOOT_RESULT *)(UINTN)BRIDGE_VM_PC_BOOT_RESULT_GPA)

UINT64
BridgeVmPcArchitecturalProtocols (VOID);

EFI_STATUS
BridgeVmPcConnectAll (VOID);

VOID
BridgeVmPcRecordGraphics (VOID);

#endif
