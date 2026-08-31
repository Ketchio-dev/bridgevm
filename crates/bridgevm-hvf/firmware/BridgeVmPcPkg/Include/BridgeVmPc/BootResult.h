/** @file
  BridgeVM Virtual ARM PC BDS and ExitBootServices result contract.
  SPDX-License-Identifier: Apache-2.0
**/
#ifndef BRIDGE_VM_PC_BOOT_RESULT_H_
#define BRIDGE_VM_PC_BOOT_RESULT_H_

#include <Uefi.h>
#include <BridgeVmPc/BootArch.h>

#define BRIDGE_VM_PC_BOOT_RESULT_GPA     0x100003000ULL
#define BRIDGE_VM_PC_BOOT_RESULT_MAGIC   0x544F4F4250434D42ULL
#define BRIDGE_VM_PC_BOOT_RESULT_VERSION 2U

#define BRIDGE_VM_PC_BOOT_STAGE_BDS_INSTALLED      1U
#define BRIDGE_VM_PC_BOOT_STAGE_BDS_ENTERED        2U
#define BRIDGE_VM_PC_BOOT_STAGE_ARCH_READY         3U
#define BRIDGE_VM_PC_BOOT_STAGE_STORAGE_CONNECTED  4U
#define BRIDGE_VM_PC_BOOT_STAGE_FILESYSTEM_FOUND   5U
#define BRIDGE_VM_PC_BOOT_STAGE_IMAGE_LOADED       6U
#define BRIDGE_VM_PC_BOOT_STAGE_READY_TO_BOOT      7U
#define BRIDGE_VM_PC_BOOT_STAGE_APPLICATION_ENTRY  8U
#define BRIDGE_VM_PC_BOOT_STAGE_MEMORY_MAP_READY   9U
#define BRIDGE_VM_PC_BOOT_STAGE_EXIT_BOOT_SERVICES 10U
#define BRIDGE_VM_PC_BOOT_STAGE_POST_EXIT           11U
#define BRIDGE_VM_PC_BOOT_STAGE_ERROR               0x80000000U

typedef struct {
  UINT64 Magic;
  UINT32 Version;
  UINT32 Stage;
  UINT64 Status;
  UINT64 ArchitecturalProtocols;
  UINT64 FileSystemCount;
  UINT64 FileSystemHandle;
  UINT64 BootImageHandle;
  UINT64 ImageBase;
  UINT64 ImageSize;
  UINT64 MemoryMapSize;
  UINT64 MapKey;
  UINT64 DescriptorSize;
  UINT32 DescriptorVersion;
  UINT32 ExitBootServicesAttempts;
  UINT64 SystemTable;
  UINT64 BootServices;
  UINT64 GopHandles;
  UINT64 FrameBufferBase;
  UINT64 FrameBufferSize;
} BRIDGE_VM_PC_BOOT_RESULT;

#endif
