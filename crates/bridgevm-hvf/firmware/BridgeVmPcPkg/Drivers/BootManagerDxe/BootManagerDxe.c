/** @file
  BridgeVM-owned removable-media BDS policy for the independent board.
  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Guid/EventGroup.h>
#include <Protocol/Bds.h>
#include <Protocol/LoadedImage.h>
#include <Protocol/SimpleFileSystem.h>
#include <Library/BaseLib.h>
#include <Library/DevicePathLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiLib.h>
#include <BridgeVmPc/BootServiceTrace.h>
#include <BridgeVmPc/StartImageFailure.h>
#include "BootManagerDxe.h"
STATIC EFI_HANDLE mImageHandle;

STATIC volatile BRIDGE_VM_PC_BOOT_RESULT *
Result (VOID)
{
  return BRIDGE_VM_PC_RESULT ();
}

STATIC VOID
Stop (IN UINT32 Stage, IN EFI_STATUS Status)
{
  Result ()->Status = Status;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_ERROR | Stage;
  __asm__ __volatile__("dsb sy\n\thvc #1" ::: "memory");
  CpuDeadLoop ();
}

STATIC VOID EFIAPI
Boot (IN EFI_BDS_ARCH_PROTOCOL *This)
{
  EFI_DEVICE_PATH_PROTOCOL *Path;
  EFI_HANDLE *FileSystems;
  EFI_LOADED_IMAGE_PROTOCOL *Loaded;
  EFI_HANDLE BootImage;
  EFI_STATUS Status;
  UINTN Count;
  UINTN Index;
  (VOID)This;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_BDS_ENTERED;
  Result ()->ArchitecturalProtocols = BridgeVmPcArchitecturalProtocols ();
  if (Result ()->ArchitecturalProtocols != BRIDGE_VM_PC_ARCH_REQUIRED) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_ARCH_READY, EFI_NOT_FOUND);
  }
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_ARCH_READY;
  Status = EfiEventGroupSignal (&gEfiEndOfDxeEventGroupGuid);
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_ARCH_READY, Status);
  }
  Status = gBS->SetWatchdogTimer (0, 0, 0, NULL);
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_ARCH_READY, Status);
  }
  Status = BridgeVmPcConnectAll ();
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_STORAGE_CONNECTED, Status);
  }
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_STORAGE_CONNECTED;
  BridgeVmPcRecordGraphics ();
  FileSystems = NULL;
  Count = 0;
  Status = gBS->LocateHandleBuffer (ByProtocol, &gEfiSimpleFileSystemProtocolGuid,
                                    NULL, &Count, &FileSystems);
  if (EFI_ERROR (Status) || (Count == 0)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_FILESYSTEM_FOUND, Status);
  }
  Result ()->FileSystemCount = Count;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_FILESYSTEM_FOUND;
  BootImage = NULL;
  Status = EFI_NOT_FOUND;
  for (Index = 0; Index < Count; ++Index) {
    Path = FileDevicePath (FileSystems[Index], L"\\EFI\\BOOT\\BOOTAA64.EFI");
    if (Path == NULL) {
      Status = EFI_OUT_OF_RESOURCES;
      break;
    }
    Status = gBS->LoadImage (TRUE, mImageHandle, Path, NULL, 0, &BootImage);
    FreePool (Path);
    if (!EFI_ERROR (Status)) {
      Result ()->FileSystemHandle = (UINT64)(UINTN)FileSystems[Index];
      break;
    }
  }
  FreePool (FileSystems);
  if (EFI_ERROR (Status) || (BootImage == NULL)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_IMAGE_LOADED, Status);
  }
  Result ()->BootImageHandle = (UINT64)(UINTN)BootImage;
  Loaded = NULL;
  Status = gBS->HandleProtocol (BootImage, &gEfiLoadedImageProtocolGuid,
                                (VOID **)&Loaded);
  if (EFI_ERROR (Status) || (Loaded == NULL)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_IMAGE_LOADED, Status);
  }
  Result ()->ImageBase = (UINT64)(UINTN)Loaded->ImageBase;
  Result ()->ImageSize = Loaded->ImageSize;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_IMAGE_LOADED;
  EfiSignalEventReadyToBoot ();
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_READY_TO_BOOT;
  BridgeVmPcArmBootServiceTrace ();
  Status = BridgeVmPcStartImageAndRecord (BootImage);
  Stop (BRIDGE_VM_PC_BOOT_STAGE_APPLICATION_ENTRY, Status);
}

STATIC EFI_BDS_ARCH_PROTOCOL mBds = { Boot };

EFI_STATUS EFIAPI
BridgeVmPcBootManagerEntry (
  IN EFI_HANDLE ImageHandle,
  IN EFI_SYSTEM_TABLE *SystemTable
  )
{
  EFI_HANDLE Handle = NULL;
  EFI_STATUS Status;
  mImageHandle = ImageHandle;
  Result ()->Magic = BRIDGE_VM_PC_BOOT_RESULT_MAGIC;
  Result ()->Version = BRIDGE_VM_PC_BOOT_RESULT_VERSION;
  Result ()->Stage = BRIDGE_VM_PC_BOOT_STAGE_BDS_INSTALLED;
  Result ()->Status = EFI_SUCCESS;
  Result ()->SystemTable = (UINT64)(UINTN)SystemTable;
  Result ()->BootServices = (UINT64)(UINTN)SystemTable->BootServices;
  Status = gBS->InstallProtocolInterface (&Handle, &gEfiBdsArchProtocolGuid,
                                          EFI_NATIVE_INTERFACE, &mBds);
  if (EFI_ERROR (Status)) {
    Stop (BRIDGE_VM_PC_BOOT_STAGE_BDS_INSTALLED, Status);
  }
  return EFI_SUCCESS;
}
