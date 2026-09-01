/** @file
  BridgeVM boot-info v1 firmware-consumer ABI.

  SPDX-License-Identifier: Apache-2.0
**/

#ifndef BRIDGE_VM_PC_BOOT_INFO_H_
#define BRIDGE_VM_PC_BOOT_INFO_H_

#include <Uefi.h>

#define BRIDGE_VM_PC_BOOT_INFO_BASE           0x26000000ULL
#define BRIDGE_VM_PC_BOOT_INFO_SIZE           0x00010000U
#define BRIDGE_VM_PC_BOOT_INFO_ABI            1U
#define BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE    112U
#define BRIDGE_VM_PC_BOOT_INFO_RSDP           0x26001000ULL
#define BRIDGE_VM_PC_BOOT_INFO_ACPI           0x26002000ULL
#define BRIDGE_VM_PC_BOOT_INFO_SMBIOS_ANCHOR  0x2600C000ULL
#define BRIDGE_VM_PC_BOOT_INFO_SMBIOS_TABLES  0x2600D000ULL
#define BRIDGE_VM_PC_RAM_BASE                 0x100000000ULL
#define BRIDGE_VM_PC_MAX_CPUS                 64U

#define BRIDGE_VM_PC_BOOT_INFO_MAGIC  SIGNATURE_64 ('B', 'V', 'M', 'B', 'O', 'O', 'T', '1')
#define BRIDGE_VM_PC_BOOT_INFO_VALID  BIT0

#pragma pack (1)
typedef struct {
  UINT64    Magic;
  UINT32    AbiVersion;
  UINT32    HeaderSize;
  UINT32    ImageSize;
  UINT8     HeaderChecksum;
  UINT8     Flags;
  UINT16    Reserved0;
  UINT64    RsdpGpa;
  UINT32    RsdpLength;
  UINT32    Reserved1;
  UINT64    AcpiTablesGpa;
  UINT32    AcpiTablesLength;
  UINT32    Reserved2;
  UINT64    SmbiosAnchorGpa;
  UINT32    SmbiosAnchorLength;
  UINT32    Reserved3;
  UINT64    SmbiosTablesGpa;
  UINT32    SmbiosTablesLength;
  UINT32    Reserved4;
  UINT64    RamBase;
  UINT64    RamSize;
  UINT32    CpuCount;
  UINT32    Reserved5;
} BRIDGE_VM_PC_BOOT_INFO;
#pragma pack ()

STATIC_ASSERT (
  sizeof (BRIDGE_VM_PC_BOOT_INFO) == BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE,
  "BridgeVM boot-info v1 header size changed"
  );

EFI_STATUS
BridgeVmPcValidateBootInfo (
  OUT CONST BRIDGE_VM_PC_BOOT_INFO  **BootInfo
  );

#endif
