/** @file Private interfaces for the BridgeVM platform-table DXE driver.
  SPDX-License-Identifier: Apache-2.0 **/
#ifndef BRIDGE_VM_PC_PLATFORM_TABLES_DXE_H_
#define BRIDGE_VM_PC_PLATFORM_TABLES_DXE_H_
#include <Uefi.h>
#include <BridgeVmPc/BootInfo.h>
EFI_STATUS
BridgeVmPcValidateAcpi (
  IN CONST BRIDGE_VM_PC_BOOT_INFO  *BootInfo
  );

EFI_STATUS
BridgeVmPcRegisterVariableStoreRuntimeMemory (
  IN EFI_SYSTEM_TABLE  *SystemTable
  );

#endif
