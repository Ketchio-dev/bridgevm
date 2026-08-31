/** @file
  BridgeVM-owned UEFI Graphics Output producer for the independent board.

  Publishes one linear 32-bit BGRx frame buffer allocated from reserved
  system RAM so the operating system never reuses it after
  ExitBootServices, as the UEFI specification and the public Microsoft
  Windows UEFI requirements demand of a boot display.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/DevicePath.h>
#include <Library/BaseMemoryLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include "GraphicsOutputDxe.h"

#pragma pack(1)
typedef struct {
  VENDOR_DEVICE_PATH Vendor;
  EFI_DEVICE_PATH_PROTOCOL End;
} BRIDGE_VM_PC_GOP_DEVICE_PATH;
#pragma pack()

STATIC BRIDGE_VM_PC_GOP_DEVICE_PATH mDevicePath = {
  {
    { HARDWARE_DEVICE_PATH, HW_VENDOR_DP,
      { sizeof (VENDOR_DEVICE_PATH), 0 } },
    { 0xB9587B5F, 0x68A5, 0x4E40,
      { 0x8B, 0xB8, 0xD6, 0x06, 0xAF, 0xF5, 0x28, 0x8A } }
  },
  { END_DEVICE_PATH_TYPE, END_ENTIRE_DEVICE_PATH_SUBTYPE,
    { sizeof (EFI_DEVICE_PATH_PROTOCOL), 0 } }
};

STATIC EFI_GRAPHICS_OUTPUT_MODE_INFORMATION mInfo = {
  0, BRIDGE_VM_PC_GOP_WIDTH, BRIDGE_VM_PC_GOP_HEIGHT,
  PixelBlueGreenRedReserved8BitPerColor,
  { 0, 0, 0, 0 },
  BRIDGE_VM_PC_GOP_WIDTH
};

STATIC EFI_GRAPHICS_OUTPUT_PROTOCOL_MODE mMode = {
  1, 0, &mInfo, sizeof (mInfo), 0, 0
};

STATIC EFI_STATUS EFIAPI
QueryMode (
  IN EFI_GRAPHICS_OUTPUT_PROTOCOL *This,
  IN UINT32 ModeNumber,
  OUT UINTN *SizeOfInfo,
  OUT EFI_GRAPHICS_OUTPUT_MODE_INFORMATION **Info
  )
{
  EFI_STATUS Status;
  (VOID)This;
  if ((ModeNumber != 0) || (SizeOfInfo == NULL) || (Info == NULL)) {
    return EFI_INVALID_PARAMETER;
  }
  Status = gBS->AllocatePool (EfiBootServicesData, sizeof (mInfo),
                              (VOID **)Info);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  CopyMem (*Info, &mInfo, sizeof (mInfo));
  *SizeOfInfo = sizeof (mInfo);
  return EFI_SUCCESS;
}

STATIC EFI_STATUS EFIAPI
SetMode (
  IN EFI_GRAPHICS_OUTPUT_PROTOCOL *This,
  IN UINT32 ModeNumber
  )
{
  if (ModeNumber != 0) {
    return EFI_UNSUPPORTED;
  }
  SetMem ((VOID *)(UINTN)This->Mode->FrameBufferBase,
          This->Mode->FrameBufferSize, 0);
  This->Mode->Mode = 0;
  return EFI_SUCCESS;
}

STATIC EFI_GRAPHICS_OUTPUT_PROTOCOL mGop = {
  QueryMode, SetMode, BridgeVmPcGopBlt, &mMode
};

EFI_STATUS EFIAPI
BridgeVmPcGraphicsOutputEntry (
  IN EFI_HANDLE ImageHandle,
  IN EFI_SYSTEM_TABLE *SystemTable
  )
{
  EFI_PHYSICAL_ADDRESS Base;
  EFI_HANDLE Handle;
  EFI_STATUS Status;
  (VOID)ImageHandle;
  (VOID)SystemTable;
  Base = 0;
  Status = gBS->AllocatePages (AllocateAnyPages, EfiReservedMemoryType,
                               EFI_SIZE_TO_PAGES (BRIDGE_VM_PC_GOP_FRAME_BYTES),
                               &Base);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  SetMem ((VOID *)(UINTN)Base, BRIDGE_VM_PC_GOP_FRAME_BYTES, 0);
  mMode.FrameBufferBase = Base;
  mMode.FrameBufferSize = BRIDGE_VM_PC_GOP_FRAME_BYTES;
  Handle = NULL;
  Status = gBS->InstallMultipleProtocolInterfaces (
                  &Handle,
                  &gEfiDevicePathProtocolGuid, &mDevicePath,
                  &gEfiGraphicsOutputProtocolGuid, &mGop,
                  NULL);
  if (EFI_ERROR (Status)) {
    gBS->FreePages (Base, EFI_SIZE_TO_PAGES (BRIDGE_VM_PC_GOP_FRAME_BYTES));
  }
  return Status;
}
