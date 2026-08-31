/** @file
  Bounded StartImage return diagnostic shared with the host probe.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_START_IMAGE_FAILURE_H_
#define BRIDGE_VM_PC_START_IMAGE_FAILURE_H_

#include <Uefi.h>
#include <BridgeVmPc/BootResult.h>

#define BRIDGE_VM_PC_START_FAILURE_GPA      (BRIDGE_VM_PC_BOOT_RESULT_GPA + 0x100ULL)
#define BRIDGE_VM_PC_START_FAILURE_MAGIC    0x3146495350434D42ULL
#define BRIDGE_VM_PC_START_FAILURE_VERSION  1U
#define BRIDGE_VM_PC_START_FAILURE_CAPACITY 96U

typedef struct {
  UINT64 Magic;
  UINT32 Version;
  UINT32 UnitCount;
  UINT64 Status;
  UINT64 ExitDataSize;
  UINT64 ExitDataAddress;
  CHAR16 ExitData[BRIDGE_VM_PC_START_FAILURE_CAPACITY];
} BRIDGE_VM_PC_START_FAILURE;

EFI_STATUS
BridgeVmPcStartImageAndRecord (IN EFI_HANDLE ImageHandle);

_Static_assert(sizeof(BRIDGE_VM_PC_START_FAILURE) <= 0xF00,
               "StartImage diagnostic exceeds its result page");

#endif
