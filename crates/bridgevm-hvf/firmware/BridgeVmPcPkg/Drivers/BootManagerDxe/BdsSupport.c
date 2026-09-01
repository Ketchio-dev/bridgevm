/** @file
  Connection, architectural-protocol and graphics evidence for BridgeVM BDS.
  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/GraphicsOutput.h>
#include <Library/BaseLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include "BootManagerDxe.h"

UINT64
BridgeVmPcArchitecturalProtocols (VOID)
{
  STATIC EFI_GUID *CONST Guids[] = {
    &gEfiSecurityArchProtocolGuid, &gEfiCpuArchProtocolGuid,
    &gEfiMetronomeArchProtocolGuid, &gEfiTimerArchProtocolGuid,
    &gEfiWatchdogTimerArchProtocolGuid, &gEfiRuntimeArchProtocolGuid,
    &gEfiVariableArchProtocolGuid, &gEfiVariableWriteArchProtocolGuid,
    &gEfiCapsuleArchProtocolGuid, &gEfiMonotonicCounterArchProtocolGuid,
    &gEfiResetArchProtocolGuid, &gEfiRealTimeClockArchProtocolGuid
  };
  UINT64 Mask = 0;
  UINTN Index;
  EFI_HANDLE *Handles;
  UINTN Count;
  for (Index = 0; Index < ARRAY_SIZE (Guids); ++Index) {
    Handles = NULL;
    Count = 0;
    if (!EFI_ERROR (gBS->LocateHandleBuffer (ByProtocol, Guids[Index], NULL,
                                             &Count, &Handles)) &&
        (Count != 0)) {
      Mask |= 1ULL << Index;
    }
    if (Handles != NULL) {
      FreePool (Handles);
    }
  }
  return Mask;
}

EFI_STATUS
BridgeVmPcConnectAll (VOID)
{
  EFI_HANDLE *Handles;
  UINTN Count;
  UINTN Index;
  UINTN Previous = 0;
  UINTN Pass;
  EFI_STATUS Status;
  for (Pass = 0; Pass < 8; ++Pass) {
    Handles = NULL;
    Count = 0;
    Status = gBS->LocateHandleBuffer (AllHandles, NULL, NULL, &Count, &Handles);
    if (EFI_ERROR (Status)) {
      return Status;
    }
    for (Index = 0; Index < Count; ++Index) {
      (VOID)gBS->ConnectController (Handles[Index], NULL, NULL, TRUE);
    }
    FreePool (Handles);
    if ((Pass != 0) && (Count == Previous)) {
      return EFI_SUCCESS;
    }
    Previous = Count;
  }
  return EFI_ABORTED;
}

VOID
BridgeVmPcRecordGraphics (VOID)
{
  volatile BRIDGE_VM_PC_BOOT_RESULT *Result = BRIDGE_VM_PC_RESULT ();
  EFI_GRAPHICS_OUTPUT_PROTOCOL *Gop;
  EFI_HANDLE *Handles = NULL;
  UINTN Count = 0;
  Result->GopHandles = 0;
  Result->FrameBufferBase = 0;
  Result->FrameBufferSize = 0;
  if (EFI_ERROR (gBS->LocateHandleBuffer (ByProtocol,
                                          &gEfiGraphicsOutputProtocolGuid,
                                          NULL, &Count, &Handles)) ||
      (Count == 0)) {
    return;
  }
  Result->GopHandles = Count;
  Gop = NULL;
  if (!EFI_ERROR (gBS->HandleProtocol (Handles[0],
                                       &gEfiGraphicsOutputProtocolGuid,
                                       (VOID **)&Gop)) &&
      (Gop != NULL) && (Gop->Mode != NULL)) {
    Result->FrameBufferBase = Gop->Mode->FrameBufferBase;
    Result->FrameBufferSize = Gop->Mode->FrameBufferSize;
  }
  FreePool (Handles);
}
