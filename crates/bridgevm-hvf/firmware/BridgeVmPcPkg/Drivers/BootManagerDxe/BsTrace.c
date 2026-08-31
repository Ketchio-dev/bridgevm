/** @file
  Core of the bounded UEFI service trace armed just before StartImage.

  Saves the original boot-services and GetVariable pointers, installs the
  recording wrappers (BsWrappers.c), and restores every pointer plus both
  table CRC32s inside the ExitBootServices wrapper, so an operating system
  that leaves boot services never observes a BridgeVM wrapper again.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include "BsTrace.h"

BRIDGE_VM_PC_BS_SAVED gBridgeVmPcBsSaved;

STATIC volatile BRIDGE_VM_PC_BS_TRACE *
Ring (VOID)
{
  return (volatile BRIDGE_VM_PC_BS_TRACE *)(UINTN)BRIDGE_VM_PC_BS_TRACE_GPA;
}

VOID
BridgeVmPcBsRecord (IN UINT32 Id, IN UINT32 Argument, IN UINT64 Detail, IN UINT64 Status)
{
  volatile BRIDGE_VM_PC_BS_TRACE_ENTRY *Entry;
  Entry = &Ring ()->Entries[Ring ()->Count % BRIDGE_VM_PC_BS_TRACE_CAPACITY];
  Entry->Id = Id;
  Entry->Argument = Argument;
  Entry->Detail = Detail;
  Entry->Status = Status;
  Ring ()->Count = Ring ()->Count + 1;
}

STATIC VOID
Reseal (IN OUT EFI_TABLE_HEADER *Header)
{
  UINT32 Crc = 0;
  Header->CRC32 = 0;
  if (!EFI_ERROR (gBridgeVmPcBsSaved.CalculateCrc32 (Header, Header->HeaderSize, &Crc))) {
    Header->CRC32 = Crc;
  }
}

VOID
BridgeVmPcBsRestore (VOID)
{
  gBS->HandleProtocol = gBridgeVmPcBsSaved.HandleProtocol;
  gBS->OpenProtocol = gBridgeVmPcBsSaved.OpenProtocol;
  gBS->LocateProtocol = gBridgeVmPcBsSaved.LocateProtocol;
  gBS->LocateHandleBuffer = gBridgeVmPcBsSaved.LocateHandleBuffer;
  gBS->LocateHandle = gBridgeVmPcBsSaved.LocateHandle;
  gBS->GetMemoryMap = gBridgeVmPcBsSaved.GetMemoryMap;
  gBS->AllocatePages = gBridgeVmPcBsSaved.AllocatePages;
  gBS->AllocatePool = gBridgeVmPcBsSaved.AllocatePool;
  gBS->Exit = gBridgeVmPcBsSaved.Exit;
  gBS->SetWatchdogTimer = gBridgeVmPcBsSaved.SetWatchdogTimer;
  gBS->ExitBootServices = gBridgeVmPcBsSaved.ExitBootServices;
  gBS->RaiseTPL = gBridgeVmPcBsSaved.RaiseTpl;
  gBS->RestoreTPL = gBridgeVmPcBsSaved.RestoreTpl;
  gBS->CreateEvent = gBridgeVmPcBsSaved.CreateEvent;
  gBS->Stall = gBridgeVmPcBsSaved.Stall;
  gBS->SetMem = gBridgeVmPcBsSaved.SetMem;
  gBS->CopyMem = gBridgeVmPcBsSaved.CopyMem;
  gBS->InstallConfigurationTable = gBridgeVmPcBsSaved.InstallConfigurationTable;
  gRT->GetVariable = gBridgeVmPcBsSaved.GetVariable;
  Reseal (&gBS->Hdr);
  Reseal (&gRT->Hdr);
}

VOID
BridgeVmPcArmBootServiceTrace (VOID)
{
  volatile BRIDGE_VM_PC_BS_TRACE *Trace = Ring ();
  UINTN Index;
  Trace->Magic = BRIDGE_VM_PC_BS_TRACE_MAGIC;
  Trace->Version = BRIDGE_VM_PC_BS_TRACE_VERSION;
  Trace->Count = 0;
  for (Index = 0; Index < BRIDGE_VM_PC_BS_TRACE_CAPACITY; ++Index) {
    Trace->Entries[Index].Id = 0;
    Trace->Entries[Index].Argument = 0;
    Trace->Entries[Index].Detail = 0;
    Trace->Entries[Index].Status = 0;
  }
  gBridgeVmPcBsSaved.CalculateCrc32 = gBS->CalculateCrc32;
  gBridgeVmPcBsSaved.HandleProtocol = gBS->HandleProtocol;
  gBridgeVmPcBsSaved.OpenProtocol = gBS->OpenProtocol;
  gBridgeVmPcBsSaved.LocateProtocol = gBS->LocateProtocol;
  gBridgeVmPcBsSaved.LocateHandleBuffer = gBS->LocateHandleBuffer;
  gBridgeVmPcBsSaved.LocateHandle = gBS->LocateHandle;
  gBridgeVmPcBsSaved.GetMemoryMap = gBS->GetMemoryMap;
  gBridgeVmPcBsSaved.AllocatePages = gBS->AllocatePages;
  gBridgeVmPcBsSaved.AllocatePool = gBS->AllocatePool;
  gBridgeVmPcBsSaved.Exit = gBS->Exit;
  gBridgeVmPcBsSaved.SetWatchdogTimer = gBS->SetWatchdogTimer;
  gBridgeVmPcBsSaved.ExitBootServices = gBS->ExitBootServices;
  gBridgeVmPcBsSaved.RaiseTpl = gBS->RaiseTPL;
  gBridgeVmPcBsSaved.RestoreTpl = gBS->RestoreTPL;
  gBridgeVmPcBsSaved.CreateEvent = gBS->CreateEvent;
  gBridgeVmPcBsSaved.Stall = gBS->Stall;
  gBridgeVmPcBsSaved.SetMem = gBS->SetMem;
  gBridgeVmPcBsSaved.CopyMem = gBS->CopyMem;
  gBridgeVmPcBsSaved.InstallConfigurationTable = gBS->InstallConfigurationTable;
  gBridgeVmPcBsSaved.GetVariable = gRT->GetVariable;
  BridgeVmPcBsInstallWrappers ();
  Reseal (&gBS->Hdr);
  Reseal (&gRT->Hdr);
}
