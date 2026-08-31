// Enter the bounded RuntimeDxe probe. SPDX-License-Identifier: Apache-2.0
#include <Uefi.h>
#include "Result.h"
#include "RuntimeProbe.h"
EFI_STATUS EFIAPI
BridgeVmPcDxeProbeEntry (
  IN EFI_HANDLE ImageHandle, IN EFI_SYSTEM_TABLE *SystemTable
  )
{
  volatile BRIDGE_VM_PC_DXE_RESULT *Result; EFI_STATUS Status;
  (VOID)ImageHandle;
  Status = BridgeVmPcRunRuntimeProbe (SystemTable);
  if (EFI_ERROR (Status)) {
    Result = (volatile BRIDGE_VM_PC_DXE_RESULT *)(UINTN)BRIDGE_VM_PC_DXE_RESULT_GPA;
    Result->Stage = 0x80000000U | (UINT32)Status;
    __asm__ __volatile__("dsb sy\n\thvc #1" ::: "memory");
  }
  return Status;
}
