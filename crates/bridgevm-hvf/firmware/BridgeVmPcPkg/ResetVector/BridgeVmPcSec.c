/** @file
  BridgeVM Virtual ARM PC bounded SEC entry.

  SPDX-License-Identifier: Apache-2.0
**/

#include "BridgeVmPcSec.h"

static uint32_t
RangeIsInsideBootInfo(uint64_t Base, uint32_t Length)
{
  return Length != 0 && Base >= BRIDGE_VM_PC_BOOT_INFO_BASE &&
         Base < BRIDGE_VM_PC_BOOT_INFO_END &&
         Length <= BRIDGE_VM_PC_BOOT_INFO_END - Base;
}

static uint32_t
HeaderChecksumIsZero(const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo)
{
  const volatile uint8_t *Bytes = (const volatile uint8_t *)BootInfo;
  uint8_t Sum = 0;
  uint32_t Index;

  for (Index = 0; Index < BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE; ++Index) {
    Sum = (uint8_t)(Sum + Bytes[Index]);
  }
  return Sum == 0;
}

uint32_t
BridgeVmPcSecMain(const volatile BRIDGE_VM_PC_BOOT_INFO *BootInfo)
{
  if (BootInfo->Magic != BRIDGE_VM_PC_BOOT_INFO_MAGIC) {
    return BRIDGE_VM_PC_SEC_BAD_MAGIC;
  }
  if (BootInfo->AbiVersion != BRIDGE_VM_PC_BOOT_INFO_ABI ||
      BootInfo->HeaderSize != BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE ||
      BootInfo->ImageSize != BRIDGE_VM_PC_BOOT_INFO_IMAGE_SIZE ||
      BootInfo->Flags != BRIDGE_VM_PC_BOOT_INFO_VALID ||
      BootInfo->Reserved0 != 0 || BootInfo->Reserved1 != 0 ||
      BootInfo->Reserved2 != 0 || BootInfo->Reserved3 != 0 ||
      BootInfo->Reserved4 != 0 || BootInfo->Reserved5 != 0) {
    return BRIDGE_VM_PC_SEC_BAD_SHAPE;
  }
  if (!HeaderChecksumIsZero(BootInfo)) {
    return BRIDGE_VM_PC_SEC_BAD_CHECKSUM;
  }
  if (BootInfo->RsdpGpa != BRIDGE_VM_PC_RSDP_GPA ||
      !RangeIsInsideBootInfo(BootInfo->RsdpGpa, BootInfo->RsdpLength) ||
      BootInfo->AcpiTablesGpa != BRIDGE_VM_PC_ACPI_GPA ||
      !RangeIsInsideBootInfo(BootInfo->AcpiTablesGpa, BootInfo->AcpiTablesLength) ||
      BootInfo->SmbiosAnchorGpa != BRIDGE_VM_PC_SMBIOS_ANCHOR_GPA ||
      !RangeIsInsideBootInfo(BootInfo->SmbiosAnchorGpa, BootInfo->SmbiosAnchorLength) ||
      BootInfo->SmbiosTablesGpa != BRIDGE_VM_PC_SMBIOS_TABLES_GPA ||
      !RangeIsInsideBootInfo(BootInfo->SmbiosTablesGpa, BootInfo->SmbiosTablesLength)) {
    return BRIDGE_VM_PC_SEC_BAD_TABLE_RANGE;
  }
  if (BootInfo->RamBase != BRIDGE_VM_PC_RAM_BASE ||
      BootInfo->RamSize < BRIDGE_VM_PC_STACK_TOP - BRIDGE_VM_PC_RAM_BASE ||
      BootInfo->RamSize > BRIDGE_VM_PC_HIGH_MMIO_BASE - BRIDGE_VM_PC_RAM_BASE ||
      BootInfo->CpuCount == 0 || BootInfo->CpuCount > BRIDGE_VM_PC_MAX_CPUS) {
    return BRIDGE_VM_PC_SEC_BAD_MACHINE;
  }
  return BRIDGE_VM_PC_SEC_SUCCESS;
}
