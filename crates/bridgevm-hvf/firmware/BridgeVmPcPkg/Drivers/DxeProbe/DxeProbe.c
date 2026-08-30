/** @file
  Bounded live marker dispatched only after BridgeVM PC enters DXE Core.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>

#define BRIDGE_VM_PC_DXE_RESULT_GPA  0x100002000ULL
#define BRIDGE_VM_PC_DXE_DISPATCHED  8U

typedef struct {
  UINT32 Stage;
  UINT32 Reserved;
  UINT64 SystemTable;
} BRIDGE_VM_PC_DXE_RESULT;

EFI_STATUS
EFIAPI
BridgeVmPcDxeProbeEntry(
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  volatile BRIDGE_VM_PC_DXE_RESULT *Result;

  (void)ImageHandle;
  Result = (volatile BRIDGE_VM_PC_DXE_RESULT *)(UINTN)BRIDGE_VM_PC_DXE_RESULT_GPA;
  Result->Reserved = 0;
  Result->SystemTable = (UINT64)(UINTN)SystemTable;
  Result->Stage = BRIDGE_VM_PC_DXE_DISPATCHED;
  __asm__ __volatile__("dsb sy\n\thvc #0" ::: "memory");
  return EFI_DEVICE_ERROR;
}
