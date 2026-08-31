/** @file
  Point gST->ConOut at the graphics console the platform actually renders.

  ConSplitter installs a virtual console aggregator, but with no console
  device selected it is a sink. After the drivers are connected, the
  GraphicsConsole driver has produced a Simple Text Output protocol on the
  GOP handle; wiring gST->ConOut to it (and resealing the system-table CRC)
  gives a loaded boot application a console that actually draws on the GOP.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/GraphicsOutput.h>
#include <Protocol/SimpleTextOut.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include "BootManagerDxe.h"

VOID
BridgeVmPcWireGraphicsConsole (VOID)
{
  EFI_HANDLE *Handles = NULL;
  UINTN Count = 0;
  EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *TextOut;
  UINT32 Crc;
  if (EFI_ERROR (gBS->LocateHandleBuffer (ByProtocol, &gEfiGraphicsOutputProtocolGuid,
                                          NULL, &Count, &Handles)) || (Count == 0)) {
    return;
  }
  if (!EFI_ERROR (gBS->HandleProtocol (Handles[0], &gEfiSimpleTextOutProtocolGuid,
                                       (VOID **)&TextOut))) {
    gST->ConsoleOutHandle = Handles[0];
    gST->ConOut = TextOut;
    gST->StandardErrorHandle = Handles[0];
    gST->StdErr = TextOut;
    gST->Hdr.CRC32 = 0;
    if (!EFI_ERROR (gBS->CalculateCrc32 (gST, gST->Hdr.HeaderSize, &Crc))) {
      gST->Hdr.CRC32 = Crc;
    }
  }
  FreePool (Handles);
}
