/** @file
  A synthetic ConIn key source for BridgeVM Virtual ARM PC bring-up.

  The board has no keyboard device yet, so gST->ConIn points at ConSplitter's
  aggregator, which has no child and therefore never yields a key. A loaded boot
  application that waits on ConIn (a boot menu, a "press a key" prompt) then
  blocks forever even though the console and timer work. This installs a minimal
  Simple Text Input protocol whose WaitForKey event is always signaled and whose
  ReadKeyStroke yields Enter, then repoints gST->ConIn at it, so a waiting boot
  application is released with an "accept the default / continue" keystroke. It
  is a bring-up scaffold; a real HID keyboard replaces it later.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Protocol/SimpleTextIn.h>
#include <Library/UefiBootServicesTableLib.h>
#include "BootManagerDxe.h"

STATIC EFI_STATUS EFIAPI
InputReset (IN EFI_SIMPLE_TEXT_INPUT_PROTOCOL *This, IN BOOLEAN ExtendedVerification)
{
  (VOID)This;
  (VOID)ExtendedVerification;
  return EFI_SUCCESS;
}

STATIC EFI_STATUS EFIAPI
InputReadKeyStroke (IN EFI_SIMPLE_TEXT_INPUT_PROTOCOL *This, OUT EFI_INPUT_KEY *Key)
{
  (VOID)This;
  if (Key == NULL) {
    return EFI_INVALID_PARAMETER;
  }
  Key->ScanCode = SCAN_NULL;
  Key->UnicodeChar = CHAR_CARRIAGE_RETURN;
  return EFI_SUCCESS;
}

STATIC VOID EFIAPI
InputWaitForKey (IN EFI_EVENT Event, IN VOID *Context)
{
  (VOID)Context;
  // A key is always available, so keep the wait event signaled.
  gBS->SignalEvent (Event);
}

STATIC EFI_SIMPLE_TEXT_INPUT_PROTOCOL mBridgeVmPcConIn = {
  InputReset,
  InputReadKeyStroke,
  NULL
};

VOID
BridgeVmPcInstallSyntheticConIn (VOID)
{
  EFI_HANDLE Handle = NULL;
  UINT32 Crc;
  if (EFI_ERROR (gBS->CreateEvent (EVT_NOTIFY_WAIT, TPL_NOTIFY, InputWaitForKey,
                                   NULL, &mBridgeVmPcConIn.WaitForKey))) {
    return;
  }
  if (EFI_ERROR (gBS->InstallProtocolInterface (&Handle, &gEfiSimpleTextInProtocolGuid,
                                                EFI_NATIVE_INTERFACE, &mBridgeVmPcConIn))) {
    return;
  }
  gST->ConsoleInHandle = Handle;
  gST->ConIn = &mBridgeVmPcConIn;
  gST->Hdr.CRC32 = 0;
  if (!EFI_ERROR (gBS->CalculateCrc32 (gST, gST->Hdr.HeaderSize, &Crc))) {
    gST->Hdr.CRC32 = Crc;
  }
}
