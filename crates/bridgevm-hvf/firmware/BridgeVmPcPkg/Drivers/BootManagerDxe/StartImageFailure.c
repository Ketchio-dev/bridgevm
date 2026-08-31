/** @file
  Preserve bounded UEFI ExitData when a loaded boot image returns.
  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/LoadedImage.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <BridgeVmPc/StartImageFailure.h>

EFI_STATUS
BridgeVmPcStartImageAndRecord (IN EFI_HANDLE ImageHandle)
{
  volatile BRIDGE_VM_PC_START_FAILURE *Record;
  VOID *Loaded = NULL;
  CHAR16 *ExitData = NULL;
  EFI_STATUS Status;
  UINTN ExitDataSize = 0;
  UINTN Limit, Index;
  Status = gBS->StartImage (ImageHandle, &ExitDataSize, &ExitData);
  Record = (volatile BRIDGE_VM_PC_START_FAILURE *)(UINTN)
           BRIDGE_VM_PC_START_FAILURE_GPA;
  Record->Magic = BRIDGE_VM_PC_START_FAILURE_MAGIC;
  Record->Version = BRIDGE_VM_PC_START_FAILURE_VERSION;
  Record->UnitCount = 0;
  Record->Status = Status;
  Record->ExitDataSize = ExitDataSize;
  Record->ExitDataAddress = (UINT64)(UINTN)ExitData;
  Record->LoadedImageProbeStatus = gBS->HandleProtocol (ImageHandle, &gEfiLoadedImageProtocolGuid, &Loaded);
  for (Index = 0; Index < BRIDGE_VM_PC_START_FAILURE_CAPACITY; ++Index) {
    Record->ExitData[Index] = 0;
  }
  if (ExitData != NULL) {
    Limit = ExitDataSize / sizeof(CHAR16);
    if (Limit > BRIDGE_VM_PC_START_FAILURE_CAPACITY) {
      Limit = BRIDGE_VM_PC_START_FAILURE_CAPACITY;
    }
    for (Index = 0; (Index < Limit) && (ExitData[Index] != L'\0'); ++Index) {
      Record->ExitData[Index] = ExitData[Index];
    }
    Record->UnitCount = (UINT32)Index;
    FreePool (ExitData);
  }
  return Status;
}
