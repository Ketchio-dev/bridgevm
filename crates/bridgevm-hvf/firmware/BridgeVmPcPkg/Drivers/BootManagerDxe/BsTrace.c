/** @file
  Bounded recorders around the UEFI services a loaded boot image calls.

  Armed once, immediately before StartImage. Every wrapper appends one
  fixed-size entry to a ring inside the shared result page and forwards to
  the saved original; the ExitBootServices wrapper restores every original
  pointer and both table CRCs before the transition, so an operating system
  that leaves boot services never sees a BridgeVM wrapper again.

  SPDX-License-Identifier: Apache-2.0
**/
#include <Uefi.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include <BridgeVmPc/BootServiceTrace.h>

STATIC EFI_HANDLE_PROTOCOL mHandleProtocol;
STATIC EFI_OPEN_PROTOCOL mOpenProtocol;
STATIC EFI_LOCATE_PROTOCOL mLocateProtocol;
STATIC EFI_LOCATE_HANDLE_BUFFER mLocateHandleBuffer;
STATIC EFI_LOCATE_HANDLE mLocateHandle;
STATIC EFI_GET_MEMORY_MAP mGetMemoryMap;
STATIC EFI_ALLOCATE_PAGES mAllocatePages;
STATIC EFI_ALLOCATE_POOL mAllocatePool;
STATIC EFI_EXIT mExit;
STATIC EFI_SET_WATCHDOG_TIMER mSetWatchdogTimer;
STATIC EFI_EXIT_BOOT_SERVICES mExitBootServices;
STATIC EFI_GET_VARIABLE mGetVariable;
STATIC EFI_CALCULATE_CRC32 mCalculateCrc32;

STATIC volatile BRIDGE_VM_PC_BS_TRACE *
Ring (VOID)
{
  return (volatile BRIDGE_VM_PC_BS_TRACE *)(UINTN)BRIDGE_VM_PC_BS_TRACE_GPA;
}

STATIC VOID
Record (IN UINT32 Id, IN UINT32 Argument, IN UINT64 Detail, IN UINT64 Status)
{
  volatile BRIDGE_VM_PC_BS_TRACE_ENTRY *Entry;
  Entry = &Ring ()->Entries[Ring ()->Count % BRIDGE_VM_PC_BS_TRACE_CAPACITY];
  Entry->Id = Id;
  Entry->Argument = Argument;
  Entry->Detail = Detail;
  Entry->Status = Status;
  Ring ()->Count = Ring ()->Count + 1;
}

STATIC UINT32
GuidArgument (IN CONST EFI_GUID *Protocol)
{
  return (Protocol == NULL) ? 0 : Protocol->Data1;
}

STATIC VOID
Reseal (IN OUT EFI_TABLE_HEADER *Header)
{
  UINT32 Crc = 0;
  Header->CRC32 = 0;
  if (!EFI_ERROR (mCalculateCrc32 (Header, Header->HeaderSize, &Crc))) {
    Header->CRC32 = Crc;
  }
}

STATIC EFI_STATUS EFIAPI
TraceHandleProtocol (IN EFI_HANDLE Handle, IN EFI_GUID *Protocol, OUT VOID **Interface)
{
  EFI_STATUS Status = mHandleProtocol (Handle, Protocol, Interface);
  Record (BRIDGE_VM_PC_BS_CALL_HANDLE_PROTOCOL, GuidArgument (Protocol),
          (UINT64)(UINTN)Handle, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceOpenProtocol (IN EFI_HANDLE Handle, IN EFI_GUID *Protocol, OUT VOID **Interface OPTIONAL,
                   IN EFI_HANDLE Agent, IN EFI_HANDLE Controller, IN UINT32 Attributes)
{
  EFI_STATUS Status = mOpenProtocol (Handle, Protocol, Interface, Agent, Controller, Attributes);
  Record (BRIDGE_VM_PC_BS_CALL_OPEN_PROTOCOL, GuidArgument (Protocol),
          (UINT64)(UINTN)Handle, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateProtocol (IN EFI_GUID *Protocol, IN VOID *Registration OPTIONAL, OUT VOID **Interface)
{
  EFI_STATUS Status = mLocateProtocol (Protocol, Registration, Interface);
  Record (BRIDGE_VM_PC_BS_CALL_LOCATE_PROTOCOL, GuidArgument (Protocol), 0, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateHandleBuffer (IN EFI_LOCATE_SEARCH_TYPE SearchType, IN EFI_GUID *Protocol OPTIONAL,
                         IN VOID *SearchKey OPTIONAL, OUT UINTN *NoHandles, OUT EFI_HANDLE **Buffer)
{
  EFI_STATUS Status = mLocateHandleBuffer (SearchType, Protocol, SearchKey, NoHandles, Buffer);
  Record (BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE_BUFFER, GuidArgument (Protocol),
          (NoHandles == NULL) ? 0 : *NoHandles, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceLocateHandle (IN EFI_LOCATE_SEARCH_TYPE SearchType, IN EFI_GUID *Protocol OPTIONAL,
                   IN VOID *SearchKey OPTIONAL, IN OUT UINTN *BufferSize, OUT EFI_HANDLE *Buffer)
{
  EFI_STATUS Status = mLocateHandle (SearchType, Protocol, SearchKey, BufferSize, Buffer);
  Record (BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE, GuidArgument (Protocol),
          (BufferSize == NULL) ? 0 : *BufferSize, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceGetMemoryMap (IN OUT UINTN *MemoryMapSize, OUT EFI_MEMORY_DESCRIPTOR *MemoryMap OPTIONAL,
                   OUT UINTN *MapKey, OUT UINTN *DescriptorSize, OUT UINT32 *DescriptorVersion)
{
  EFI_STATUS Status = mGetMemoryMap (MemoryMapSize, MemoryMap, MapKey, DescriptorSize, DescriptorVersion);
  Record (BRIDGE_VM_PC_BS_CALL_GET_MEMORY_MAP, 0,
          (MemoryMapSize == NULL) ? 0 : *MemoryMapSize, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceAllocatePages (IN EFI_ALLOCATE_TYPE Type, IN EFI_MEMORY_TYPE MemoryType,
                    IN UINTN Pages, IN OUT EFI_PHYSICAL_ADDRESS *Memory)
{
  EFI_STATUS Status = mAllocatePages (Type, MemoryType, Pages, Memory);
  Record (BRIDGE_VM_PC_BS_CALL_ALLOCATE_PAGES,
          ((UINT32)Type << 8) | ((UINT32)MemoryType & 0xFF),
          (Memory == NULL) ? 0 : (UINT64)*Memory, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceAllocatePool (IN EFI_MEMORY_TYPE PoolType, IN UINTN Size, OUT VOID **Buffer)
{
  EFI_STATUS Status = mAllocatePool (PoolType, Size, Buffer);
  Record (BRIDGE_VM_PC_BS_CALL_ALLOCATE_POOL, (UINT32)PoolType, Size, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceExit (IN EFI_HANDLE ImageHandle, IN EFI_STATUS ExitStatus,
           IN UINTN ExitDataSize, IN CHAR16 *ExitData OPTIONAL)
{
  Record (BRIDGE_VM_PC_BS_CALL_EXIT, (UINT32)ExitDataSize,
          (UINT64)(UINTN)ImageHandle, ExitStatus);
  return mExit (ImageHandle, ExitStatus, ExitDataSize, ExitData);
}

STATIC EFI_STATUS EFIAPI
TraceSetWatchdogTimer (IN UINTN Timeout, IN UINT64 WatchdogCode,
                       IN UINTN DataSize, IN CHAR16 *WatchdogData OPTIONAL)
{
  EFI_STATUS Status = mSetWatchdogTimer (Timeout, WatchdogCode, DataSize, WatchdogData);
  Record (BRIDGE_VM_PC_BS_CALL_SET_WATCHDOG_TIMER, (UINT32)Timeout, WatchdogCode, Status);
  return Status;
}

STATIC EFI_STATUS EFIAPI
TraceGetVariable (IN CHAR16 *VariableName, IN EFI_GUID *VendorGuid,
                  OUT UINT32 *Attributes OPTIONAL, IN OUT UINTN *DataSize, OUT VOID *Data OPTIONAL)
{
  EFI_STATUS Status = mGetVariable (VariableName, VendorGuid, Attributes, DataSize, Data);
  UINT32 Name = 0;
  if (VariableName != NULL) {
    Name = (UINT32)VariableName[0];
    if (VariableName[0] != L'\0') {
      Name |= (UINT32)VariableName[1] << 16;
    }
  }
  Record (BRIDGE_VM_PC_BS_CALL_GET_VARIABLE, Name,
          (DataSize == NULL) ? 0 : *DataSize, Status);
  return Status;
}

STATIC VOID
Disarm (VOID)
{
  gBS->HandleProtocol = mHandleProtocol;
  gBS->OpenProtocol = mOpenProtocol;
  gBS->LocateProtocol = mLocateProtocol;
  gBS->LocateHandleBuffer = mLocateHandleBuffer;
  gBS->LocateHandle = mLocateHandle;
  gBS->GetMemoryMap = mGetMemoryMap;
  gBS->AllocatePages = mAllocatePages;
  gBS->AllocatePool = mAllocatePool;
  gBS->Exit = mExit;
  gBS->SetWatchdogTimer = mSetWatchdogTimer;
  gBS->ExitBootServices = mExitBootServices;
  gRT->GetVariable = mGetVariable;
  Reseal (&gBS->Hdr);
  Reseal (&gRT->Hdr);
}

STATIC EFI_STATUS EFIAPI
TraceExitBootServices (IN EFI_HANDLE ImageHandle, IN UINTN MapKey)
{
  EFI_STATUS (EFIAPI *Original)(EFI_HANDLE, UINTN) = mExitBootServices;
  Record (BRIDGE_VM_PC_BS_CALL_EXIT_BOOT_SERVICES, 0, MapKey, EFI_SUCCESS);
  Disarm ();
  return Original (ImageHandle, MapKey);
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
  mCalculateCrc32 = gBS->CalculateCrc32;
  mHandleProtocol = gBS->HandleProtocol;
  mOpenProtocol = gBS->OpenProtocol;
  mLocateProtocol = gBS->LocateProtocol;
  mLocateHandleBuffer = gBS->LocateHandleBuffer;
  mLocateHandle = gBS->LocateHandle;
  mGetMemoryMap = gBS->GetMemoryMap;
  mAllocatePages = gBS->AllocatePages;
  mAllocatePool = gBS->AllocatePool;
  mExit = gBS->Exit;
  mSetWatchdogTimer = gBS->SetWatchdogTimer;
  mExitBootServices = gBS->ExitBootServices;
  mGetVariable = gRT->GetVariable;
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
  gRT->GetVariable = TraceGetVariable;
  Reseal (&gBS->Hdr);
  Reseal (&gRT->Hdr);
}
