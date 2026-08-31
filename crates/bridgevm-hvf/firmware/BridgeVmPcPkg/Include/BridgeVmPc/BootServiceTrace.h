/** @file
  Bounded UEFI service-call ring recorded between ReadyToBoot and StartImage.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_BOOT_SERVICE_TRACE_H_
#define BRIDGE_VM_PC_BOOT_SERVICE_TRACE_H_

#include <BridgeVmPc/BootResult.h>

#define BRIDGE_VM_PC_BS_TRACE_GPA      (BRIDGE_VM_PC_BOOT_RESULT_GPA + 0x400ULL)
#define BRIDGE_VM_PC_BS_TRACE_MAGIC    0x4341525453424D42ULL
#define BRIDGE_VM_PC_BS_TRACE_VERSION  1U
#define BRIDGE_VM_PC_BS_TRACE_CAPACITY 126U

#define BRIDGE_VM_PC_BS_CALL_HANDLE_PROTOCOL      1U
#define BRIDGE_VM_PC_BS_CALL_OPEN_PROTOCOL        2U
#define BRIDGE_VM_PC_BS_CALL_LOCATE_PROTOCOL      3U
#define BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE_BUFFER 4U
#define BRIDGE_VM_PC_BS_CALL_LOCATE_HANDLE        5U
#define BRIDGE_VM_PC_BS_CALL_GET_MEMORY_MAP       6U
#define BRIDGE_VM_PC_BS_CALL_ALLOCATE_PAGES       7U
#define BRIDGE_VM_PC_BS_CALL_ALLOCATE_POOL        8U
#define BRIDGE_VM_PC_BS_CALL_EXIT                 9U
#define BRIDGE_VM_PC_BS_CALL_SET_WATCHDOG_TIMER   10U
#define BRIDGE_VM_PC_BS_CALL_EXIT_BOOT_SERVICES   11U
#define BRIDGE_VM_PC_BS_CALL_GET_VARIABLE         12U

typedef struct {
  UINT32 Id;
  UINT32 Argument;
  UINT64 Detail;
  UINT64 Status;
} BRIDGE_VM_PC_BS_TRACE_ENTRY;

typedef struct {
  UINT64 Magic;
  UINT32 Version;
  UINT32 Count;
  BRIDGE_VM_PC_BS_TRACE_ENTRY Entries[BRIDGE_VM_PC_BS_TRACE_CAPACITY];
} BRIDGE_VM_PC_BS_TRACE;

VOID
BridgeVmPcArmBootServiceTrace (VOID);

_Static_assert(sizeof(BRIDGE_VM_PC_BS_TRACE) <= 0xC00, "service trace exceeds its result page span");

#endif
