/** @file
  Bounded UEFI Blt implementation for the fixed BridgeVM 32-bit frame buffer.

  Written against the EFI_GRAPHICS_OUTPUT_PROTOCOL.Blt() contract in the
  UEFI specification: every video rectangle is validated before any byte
  moves, and unbounded caller state is limited to the caller's own buffer.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Library/BaseMemoryLib.h>
#include "GraphicsOutputDxe.h"

STATIC BOOLEAN
VideoRectangleIsInvalid (IN UINTN X, IN UINTN Y, IN UINTN Width, IN UINTN Height)
{
  return (X > BRIDGE_VM_PC_GOP_WIDTH) || (Width > BRIDGE_VM_PC_GOP_WIDTH - X) ||
         (Y > BRIDGE_VM_PC_GOP_HEIGHT) || (Height > BRIDGE_VM_PC_GOP_HEIGHT - Y);
}

STATIC UINT32 *
VideoPixel (IN EFI_GRAPHICS_OUTPUT_PROTOCOL *This, IN UINTN X, IN UINTN Y)
{
  return (UINT32 *)(UINTN)This->Mode->FrameBufferBase +
         (Y * BRIDGE_VM_PC_GOP_WIDTH) + X;
}

EFI_STATUS EFIAPI
BridgeVmPcGopBlt (
  IN EFI_GRAPHICS_OUTPUT_PROTOCOL *This,
  IN OUT EFI_GRAPHICS_OUTPUT_BLT_PIXEL *BltBuffer OPTIONAL,
  IN EFI_GRAPHICS_OUTPUT_BLT_OPERATION BltOperation,
  IN UINTN SourceX,
  IN UINTN SourceY,
  IN UINTN DestinationX,
  IN UINTN DestinationY,
  IN UINTN Width,
  IN UINTN Height,
  IN UINTN Delta OPTIONAL
  )
{
  UINTN Row;
  UINTN RowBytes;
  UINT8 *Caller;
  if ((This == NULL) || (Width == 0) || (Height == 0)) {
    return EFI_INVALID_PARAMETER;
  }
  RowBytes = Width * BRIDGE_VM_PC_GOP_PIXEL_BYTES;
  if (Delta == 0) {
    Delta = RowBytes;
  }
  switch (BltOperation) {
  case EfiBltVideoFill:
    if ((BltBuffer == NULL) ||
        VideoRectangleIsInvalid (DestinationX, DestinationY, Width, Height)) {
      return EFI_INVALID_PARAMETER;
    }
    for (Row = 0; Row < Height; ++Row) {
      SetMem32 (VideoPixel (This, DestinationX, DestinationY + Row),
                RowBytes, *(UINT32 *)BltBuffer);
    }
    return EFI_SUCCESS;
  case EfiBltVideoToBltBuffer:
    if ((BltBuffer == NULL) ||
        VideoRectangleIsInvalid (SourceX, SourceY, Width, Height)) {
      return EFI_INVALID_PARAMETER;
    }
    Caller = (UINT8 *)BltBuffer + (DestinationY * Delta) +
             (DestinationX * BRIDGE_VM_PC_GOP_PIXEL_BYTES);
    for (Row = 0; Row < Height; ++Row) {
      CopyMem (Caller + (Row * Delta),
               VideoPixel (This, SourceX, SourceY + Row), RowBytes);
    }
    return EFI_SUCCESS;
  case EfiBltBufferToVideo:
    if ((BltBuffer == NULL) ||
        VideoRectangleIsInvalid (DestinationX, DestinationY, Width, Height)) {
      return EFI_INVALID_PARAMETER;
    }
    Caller = (UINT8 *)BltBuffer + (SourceY * Delta) +
             (SourceX * BRIDGE_VM_PC_GOP_PIXEL_BYTES);
    for (Row = 0; Row < Height; ++Row) {
      CopyMem (VideoPixel (This, DestinationX, DestinationY + Row),
               Caller + (Row * Delta), RowBytes);
    }
    return EFI_SUCCESS;
  case EfiBltVideoToVideo:
    if (VideoRectangleIsInvalid (SourceX, SourceY, Width, Height) ||
        VideoRectangleIsInvalid (DestinationX, DestinationY, Width, Height)) {
      return EFI_INVALID_PARAMETER;
    }
    for (Row = 0; Row < Height; ++Row) {
      UINTN Line = (DestinationY > SourceY) ? (Height - 1 - Row) : Row;
      CopyMem (VideoPixel (This, DestinationX, DestinationY + Line),
               VideoPixel (This, SourceX, SourceY + Line), RowBytes);
    }
    return EFI_SUCCESS;
  default:
    return EFI_INVALID_PARAMETER;
  }
}
