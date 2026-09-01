/** @file
  BridgeVM Virtual ARM PC UEFI Graphics Output contract.

  One fixed linear mode backed by reserved system RAM so the framebuffer
  survives ExitBootServices: PixelBlueGreenRedReserved8BitPerColor with
  PixelsPerScanLine equal to the horizontal resolution.

  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_GRAPHICS_OUTPUT_DXE_H_
#define BRIDGE_VM_PC_GRAPHICS_OUTPUT_DXE_H_

#include <Uefi.h>
#include <Protocol/GraphicsOutput.h>

#define BRIDGE_VM_PC_GOP_WIDTH        1024U
#define BRIDGE_VM_PC_GOP_HEIGHT       768U
#define BRIDGE_VM_PC_GOP_PIXEL_BYTES  4U
#define BRIDGE_VM_PC_GOP_FRAME_BYTES \
  ((UINTN)BRIDGE_VM_PC_GOP_WIDTH * BRIDGE_VM_PC_GOP_HEIGHT * \
   BRIDGE_VM_PC_GOP_PIXEL_BYTES)

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
  );

#endif
