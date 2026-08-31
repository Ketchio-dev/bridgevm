/** @file
  Point gST->ConOut and gST->ConIn at the consoles the platform provides.

  ConSplitter installs virtual console aggregators, but with nothing wired
  into gST they stay NULL -- and a loaded boot application that touches the
  console then dereferences NULL. After the drivers are connected, the
  GraphicsConsole driver has produced a Simple Text Output protocol on the
  GOP handle and ConSplitter has produced a Simple Text Input aggregator;
  wiring gST->ConOut/ConIn to them (and resealing the system-table CRC once)
  gives a loaded boot application both a console that draws on the GOP and a
  non-NULL input path it can reset and poll.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/GraphicsOutput.h>
#include <Protocol/SimpleTextOut.h>
#include <Protocol/SimpleTextIn.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include "BootManagerDxe.h"

VOID
BridgeVmPcWireGraphicsConsole (VOID)
{
  EFI_HANDLE *Handles;
  UINTN Count;
  EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *TextOut;
  EFI_SIMPLE_TEXT_INPUT_PROTOCOL *TextIn;
  UINT32 Crc;

  Handles = NULL;
  Count = 0;
  if (!EFI_ERROR (gBS->LocateHandleBuffer (ByProtocol, &gEfiGraphicsOutputProtocolGuid,
                                           NULL, &Count, &Handles)) && (Count != 0) &&
      !EFI_ERROR (gBS->HandleProtocol (Handles[0], &gEfiSimpleTextOutProtocolGuid,
                                       (VOID **)&TextOut))) {
    gST->ConsoleOutHandle = Handles[0];
    gST->ConOut = TextOut;
    gST->StandardErrorHandle = Handles[0];
    gST->StdErr = TextOut;
  }
  if (Handles != NULL) {
    FreePool (Handles);
    Handles = NULL;
  }

  Count = 0;
  if (!EFI_ERROR (gBS->LocateHandleBuffer (ByProtocol, &gEfiSimpleTextInProtocolGuid,
                                           NULL, &Count, &Handles)) && (Count != 0) &&
      !EFI_ERROR (gBS->HandleProtocol (Handles[0], &gEfiSimpleTextInProtocolGuid,
                                       (VOID **)&TextIn))) {
    gST->ConsoleInHandle = Handles[0];
    gST->ConIn = TextIn;
  }
  if (Handles != NULL) {
    FreePool (Handles);
  }

  gST->Hdr.CRC32 = 0;
  if (!EFI_ERROR (gBS->CalculateCrc32 (gST, gST->Hdr.HeaderSize, &Crc))) {
    gST->Hdr.CRC32 = Crc;
  }
}
