/** @file
  Internal state shared by the BridgeVM boot-services trace and its wrappers.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_BS_TRACE_INTERNAL_H_
#define BRIDGE_VM_PC_BS_TRACE_INTERNAL_H_

#include <Uefi.h>
#include <BridgeVmPc/BootServiceTrace.h>

typedef struct {
  EFI_HANDLE_PROTOCOL HandleProtocol;
  EFI_OPEN_PROTOCOL OpenProtocol;
  EFI_LOCATE_PROTOCOL LocateProtocol;
  EFI_LOCATE_HANDLE_BUFFER LocateHandleBuffer;
  EFI_LOCATE_HANDLE LocateHandle;
  EFI_GET_MEMORY_MAP GetMemoryMap;
  EFI_ALLOCATE_PAGES AllocatePages;
  EFI_ALLOCATE_POOL AllocatePool;
  EFI_EXIT Exit;
  EFI_SET_WATCHDOG_TIMER SetWatchdogTimer;
  EFI_EXIT_BOOT_SERVICES ExitBootServices;
  EFI_GET_VARIABLE GetVariable;
  EFI_RAISE_TPL RaiseTpl;
  EFI_RESTORE_TPL RestoreTpl;
  EFI_CREATE_EVENT CreateEvent;
  EFI_STALL Stall;
  EFI_SET_MEM SetMem;
  EFI_COPY_MEM CopyMem;
  EFI_INSTALL_CONFIGURATION_TABLE InstallConfigurationTable;
  EFI_CALCULATE_CRC32 CalculateCrc32;
} BRIDGE_VM_PC_BS_SAVED;

extern BRIDGE_VM_PC_BS_SAVED gBridgeVmPcBsSaved;

VOID
BridgeVmPcBsRecord (IN UINT32 Id, IN UINT32 Argument, IN UINT64 Detail, IN UINT64 Status);

VOID
BridgeVmPcBsInstallWrappers (VOID);

VOID
BridgeVmPcBsRestore (VOID);

#endif
