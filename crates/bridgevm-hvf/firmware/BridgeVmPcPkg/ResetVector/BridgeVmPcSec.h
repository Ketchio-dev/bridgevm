/** @file
  BridgeVM Virtual ARM PC bounded SEC contract.

  SPDX-License-Identifier: Apache-2.0
**/

#ifndef BRIDGE_VM_PC_SEC_H_
#define BRIDGE_VM_PC_SEC_H_

#include <stdint.h>

#define BRIDGE_VM_PC_BOOT_INFO_MAGIC        0x31544F4F424D5642ULL
#define BRIDGE_VM_PC_BOOT_INFO_ABI          1U
#define BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE  112U
#define BRIDGE_VM_PC_BOOT_INFO_IMAGE_SIZE   0x10000U
#define BRIDGE_VM_PC_BOOT_INFO_VALID        1U
#define BRIDGE_VM_PC_BOOT_INFO_BASE         0x26000000ULL
#define BRIDGE_VM_PC_BOOT_INFO_END          0x26010000ULL
#define BRIDGE_VM_PC_RSDP_GPA               0x26001000ULL
#define BRIDGE_VM_PC_ACPI_GPA               0x26002000ULL
#define BRIDGE_VM_PC_SMBIOS_ANCHOR_GPA      0x2600C000ULL
#define BRIDGE_VM_PC_SMBIOS_TABLES_GPA      0x2600D000ULL
#define BRIDGE_VM_PC_RAM_BASE               0x100000000ULL
#define BRIDGE_VM_PC_STACK_TOP              0x100020000ULL
#define BRIDGE_VM_PC_HIGH_MMIO_BASE          0x2000000000ULL
#define BRIDGE_VM_PC_MAX_CPUS               64U

#define BRIDGE_VM_PC_SEC_SUCCESS             1U
#define BRIDGE_VM_PC_SEC_BAD_MAGIC           2U
#define BRIDGE_VM_PC_SEC_BAD_SHAPE           3U
#define BRIDGE_VM_PC_SEC_BAD_CHECKSUM        4U
#define BRIDGE_VM_PC_SEC_BAD_TABLE_RANGE     5U
#define BRIDGE_VM_PC_SEC_BAD_MACHINE         6U
#define BRIDGE_VM_PC_SEC_BAD_HOB             7U

typedef struct __attribute__((packed)) {
  uint64_t Magic;
  uint32_t AbiVersion;
  uint32_t HeaderSize;
  uint32_t ImageSize;
  uint8_t  HeaderChecksum;
  uint8_t  Flags;
  uint16_t Reserved0;
  uint64_t RsdpGpa;
  uint32_t RsdpLength;
  uint32_t Reserved1;
  uint64_t AcpiTablesGpa;
  uint32_t AcpiTablesLength;
  uint32_t Reserved2;
  uint64_t SmbiosAnchorGpa;
  uint32_t SmbiosAnchorLength;
  uint32_t Reserved3;
  uint64_t SmbiosTablesGpa;
  uint32_t SmbiosTablesLength;
  uint32_t Reserved4;
  uint64_t RamBase;
  uint64_t RamSize;
  uint32_t CpuCount;
  uint32_t Reserved5;
} BRIDGE_VM_PC_BOOT_INFO;

_Static_assert(sizeof(BRIDGE_VM_PC_BOOT_INFO) == BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE,
               "BridgeVM boot-info v1 header size changed");

#endif
