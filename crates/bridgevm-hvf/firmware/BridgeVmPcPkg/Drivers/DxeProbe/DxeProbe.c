/** @file
  Enter the bounded RuntimeDxe live probe after its architectural protocol.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include "RuntimeProbe.h"

EFI_STATUS
EFIAPI
BridgeVmPcDxeProbeEntry(
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  (VOID)ImageHandle;
  return BridgeVmPcRunRuntimeProbe (SystemTable);
}
