/** @file
  Recording wrappers for the BridgeVM boot-services trace.

  Each wrapper appends one fixed-size entry and forwards to the saved
  original. The ExitBootServices wrapper restores every pointer before the
  transition. The foundational early services (RaiseTPL, CreateEvent,
  Stall, SetMem, CopyMem) are traced so the exact point a loaded image
  stops is visible even when it never reaches the memory-map services.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include "BsTrace.h"

#define SAVED gBridgeVmPcBsSaved
#define REC   BridgeVmPcBsRecord

STATIC UINT32
GuidArg (IN CONST EFI_GUID *Protocol)
{
  return (Protocol == NULL) ? 0 : Protocol->Data1;
}

STATIC EFI_STATUS EFIAPI
TraceHandleProtocol (IN EFI_HANDLE Handle, IN EFI_GUID *Protocol, OUT VOID **Interface)
{
  EFI_STATUS Status = SAVED.HandleProtocol (Handle, Protocol, Interface);
  REC (BRIDGE_VM_PC_BS_CALL_HANDLE_PROTOCOL, GuidArg (Protocol), (UINT64)(UINTN)Handle, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceOpenProtocol (IN EFI_HANDLE Handle, IN EFI_GUID *Protocol, OUT VOID **Interface OPTIONAL,
                   IN EFI_HANDLE Agent, IN EFI_HANDLE Controller, IN UINT32 Attributes)
{
  EFI_STATUS Status = SAVED.OpenProtocol (Handle, Protocol, Interface, Agent, Controller, Attributes);
  REC (BRIDGE_VM_PC_BS_CALL_OPEN_PROTOCOL, GuidArg (Protocol), (UINT64)(UINTN)Handle, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateProtocol (IN EFI_GUID *Protocol, IN VOID *Registration OPTIONAL, OUT VOID **Interface)
{
  EFI_STATUS Status = SAVED.LocateProtocol (Protocol, Registration, Interface);
  REC (BRIDGE_VM_PC_BS_CALL_LOCATE_PROTOCOL, GuidArg (Protocol), 0, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateHandleBuffer (IN EFI_LOCATE_SEARCH_TYPE SearchType, IN EFI_GUID *Protocol OPTIONAL,
                         IN VOID *SearchKey OPTIONAL, OUT UINTN *NoHandles, OUT EFI_HANDLE **Buffer)
{
  EFI_STATUS Status = SAVED.LocateHandleBuffer (SearchType, Protocol, SearchKey, NoHandles, Buffer);
  REC (BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE_BUFFER, GuidArg (Protocol),
       (NoHandles == NULL) ? 0 : *NoHandles, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateHandle (IN EFI_LOCATE_SEARCH_TYPE SearchType, IN EFI_GUID *Protocol OPTIONAL,
                   IN VOID *SearchKey OPTIONAL, IN OUT UINTN *BufferSize, OUT EFI_HANDLE *Buffer)
{
  EFI_STATUS Status = SAVED.LocateHandle (SearchType, Protocol, SearchKey, BufferSize, Buffer);
  REC (BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE, GuidArg (Protocol),
       (BufferSize == NULL) ? 0 : *BufferSize, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceGetMemoryMap (IN OUT UINTN *MemoryMapSize, OUT EFI_MEMORY_DESCRIPTOR *MemoryMap OPTIONAL,
                   OUT UINTN *MapKey, OUT UINTN *DescriptorSize, OUT UINT32 *DescriptorVersion)
{
  EFI_STATUS Status = SAVED.GetMemoryMap (MemoryMapSize, MemoryMap, MapKey, DescriptorSize, DescriptorVersion);
  REC (BRIDGE_VM_PC_BS_CALL_GET_MEMORY_MAP, 0, (MemoryMapSize == NULL) ? 0 : *MemoryMapSize, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceAllocatePages (IN EFI_ALLOCATE_TYPE Type, IN EFI_MEMORY_TYPE MemoryType,
                    IN UINTN Pages, IN OUT EFI_PHYSICAL_ADDRESS *Memory)
{
  EFI_STATUS Status = SAVED.AllocatePages (Type, MemoryType, Pages, Memory);
  REC (BRIDGE_VM_PC_BS_CALL_ALLOCATE_PAGES, ((UINT32)Type << 8) | ((UINT32)MemoryType & 0xFF),
       (Memory == NULL) ? 0 : (UINT64)*Memory, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceAllocatePool (IN EFI_MEMORY_TYPE PoolType, IN UINTN Size, OUT VOID **Buffer)
{
  EFI_STATUS Status = SAVED.AllocatePool (PoolType, Size, Buffer);
  REC (BRIDGE_VM_PC_BS_CALL_ALLOCATE_POOL, (UINT32)PoolType, Size, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceExit (IN EFI_HANDLE ImageHandle, IN EFI_STATUS ExitStatus,
           IN UINTN ExitDataSize, IN CHAR16 *ExitData OPTIONAL)
{
  REC (BRIDGE_VM_PC_BS_CALL_EXIT, (UINT32)ExitDataSize, (UINT64)(UINTN)ImageHandle, ExitStatus);
  return SAVED.Exit (ImageHandle, ExitStatus, ExitDataSize, ExitData);
}

STATIC EFI_STATUS EFIAPI
TraceSetWatchdogTimer (IN UINTN Timeout, IN UINT64 WatchdogCode,
                       IN UINTN DataSize, IN CHAR16 *WatchdogData OPTIONAL)
{
  EFI_STATUS Status = SAVED.SetWatchdogTimer (Timeout, WatchdogCode, DataSize, WatchdogData);
  REC (BRIDGE_VM_PC_BS_CALL_SET_WATCHDOG_TIMER, (UINT32)Timeout, WatchdogCode, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceGetVariable (IN CHAR16 *VariableName, IN EFI_GUID *VendorGuid,
                  OUT UINT32 *Attributes OPTIONAL, IN OUT UINTN *DataSize, OUT VOID *Data OPTIONAL)
{
  EFI_STATUS Status = SAVED.GetVariable (VariableName, VendorGuid, Attributes, DataSize, Data);
  UINT32 Name = 0;
  if (VariableName != NULL) {
    Name = (UINT32)VariableName[0];
    if (VariableName[0] != L'\0') {
      Name |= (UINT32)VariableName[1] << 16;
    }
  }
  REC (BRIDGE_VM_PC_BS_CALL_GET_VARIABLE, Name, (DataSize == NULL) ? 0 : *DataSize, Status);
  return Status;
}

STATIC EFI_TPL EFIAPI
TraceRaiseTpl (IN EFI_TPL NewTpl)
{
  // Not recorded: a loaded image's steady WaitForEvent loop is almost all
  // RaiseTPL/RestoreTPL, which would flood the bounded ring and hide the
  // meaningful calls before the wait. Wrapper stays installed for symmetry.
  return SAVED.RaiseTpl (NewTpl);
}

STATIC VOID EFIAPI
TraceRestoreTpl (IN EFI_TPL OldTpl)
{
  SAVED.RestoreTpl (OldTpl);
}

STATIC EFI_STATUS EFIAPI
TraceCreateEvent (IN UINT32 Type, IN EFI_TPL NotifyTpl, IN EFI_EVENT_NOTIFY NotifyFunction OPTIONAL,
                  IN VOID *NotifyContext OPTIONAL, OUT EFI_EVENT *Event)
{
  EFI_STATUS Status = SAVED.CreateEvent (Type, NotifyTpl, NotifyFunction, NotifyContext, Event);
  REC (BRIDGE_VM_PC_BS_CALL_CREATE_EVENT, Type, (UINT64)NotifyTpl, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceStall (IN UINTN Microseconds)
{
  EFI_STATUS Status = SAVED.Stall (Microseconds);
  REC (BRIDGE_VM_PC_BS_CALL_STALL, 0, Microseconds, Status);
  return Status;
}

STATIC VOID EFIAPI
TraceSetMem (IN VOID *Buffer, IN UINTN Size, IN UINT8 Value)
{
  REC (BRIDGE_VM_PC_BS_CALL_SET_MEM, Value, Size, 0);
  SAVED.SetMem (Buffer, Size, Value);
}

STATIC VOID EFIAPI
TraceCopyMem (IN VOID *Destination, IN VOID *Source, IN UINTN Length)
{
  REC (BRIDGE_VM_PC_BS_CALL_COPY_MEM, 0, Length, 0);
  SAVED.CopyMem (Destination, Source, Length);
}

STATIC EFI_STATUS EFIAPI
TraceInstallConfigurationTable (IN EFI_GUID *Guid, IN VOID *Table)
{
  EFI_STATUS Status = SAVED.InstallConfigurationTable (Guid, Table);
  REC (BRIDGE_VM_PC_BS_CALL_INSTALL_CONFIG_TABLE, GuidArg (Guid), (UINT64)(UINTN)Table, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceExitBootServices (IN EFI_HANDLE ImageHandle, IN UINTN MapKey)
{
  EFI_EXIT_BOOT_SERVICES Original = SAVED.ExitBootServices;
  REC (BRIDGE_VM_PC_BS_CALL_EXIT_BOOT_SERVICES, 0, MapKey, EFI_SUCCESS);
  BridgeVmPcBsRestore ();
  return Original (ImageHandle, MapKey);
}

VOID
BridgeVmPcBsInstallWrappers (VOID)
{
  gBS->HandleProtocol = TraceHandleProtocol;
  gBS->OpenProtocol = TraceOpenProtocol;
  gBS->LocateProtocol = TraceLocateProtocol;
  gBS->LocateHandleBuffer = TraceLocateHandleBuffer;
  gBS->LocateHandle = TraceLocateHandle;
  gBS->GetMemoryMap = TraceGetMemoryMap;
  gBS->AllocatePages = TraceAllocatePages;
  gBS->AllocatePool = TraceAllocatePool;
  gBS->Exit = TraceExit;
  gBS->SetWatchdogTimer = TraceSetWatchdogTimer;
  gBS->ExitBootServices = TraceExitBootServices;
  gBS->RaiseTPL = TraceRaiseTpl;
  gBS->RestoreTPL = TraceRestoreTpl;
  gBS->CreateEvent = TraceCreateEvent;
  gBS->Stall = TraceStall;
  gBS->SetMem = TraceSetMem;
  gBS->CopyMem = TraceCopyMem;
  gBS->InstallConfigurationTable = TraceInstallConfigurationTable;
  gRT->GetVariable = TraceGetVariable;
}
