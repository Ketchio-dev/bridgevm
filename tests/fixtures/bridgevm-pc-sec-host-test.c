/* SPDX-License-Identifier: Apache-2.0 */
/* Deterministic host tests for the freestanding BridgeVM PC SEC validator. */
#include <stdio.h>
#include <string.h>
#include "BridgeVmPcSec.h"

static void
FinalizeChecksum(BRIDGE_VM_PC_BOOT_INFO *BootInfo)
{
  uint8_t *Bytes = (uint8_t *)BootInfo;
  uint8_t Sum = 0;
  uint32_t Index;

  BootInfo->HeaderChecksum = 0;
  for (Index = 0; Index < sizeof(*BootInfo); ++Index) {
    Sum = (uint8_t)(Sum + Bytes[Index]);
  }
  BootInfo->HeaderChecksum = (uint8_t)(0U - Sum);
}

static BRIDGE_VM_PC_BOOT_INFO
ValidBootInfo(void)
{
  BRIDGE_VM_PC_BOOT_INFO BootInfo = {0};

  BootInfo.Magic = BRIDGE_VM_PC_BOOT_INFO_MAGIC;
  BootInfo.AbiVersion = BRIDGE_VM_PC_BOOT_INFO_ABI;
  BootInfo.HeaderSize = BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE;
  BootInfo.ImageSize = BRIDGE_VM_PC_BOOT_INFO_IMAGE_SIZE;
  BootInfo.Flags = BRIDGE_VM_PC_BOOT_INFO_VALID;
  BootInfo.RsdpGpa = BRIDGE_VM_PC_RSDP_GPA;
  BootInfo.RsdpLength = 36;
  BootInfo.AcpiTablesGpa = BRIDGE_VM_PC_ACPI_GPA;
  BootInfo.AcpiTablesLength = 0x1000;
  BootInfo.SmbiosAnchorGpa = BRIDGE_VM_PC_SMBIOS_ANCHOR_GPA;
  BootInfo.SmbiosAnchorLength = 24;
  BootInfo.SmbiosTablesGpa = BRIDGE_VM_PC_SMBIOS_TABLES_GPA;
  BootInfo.SmbiosTablesLength = 128;
  BootInfo.RamBase = BRIDGE_VM_PC_RAM_BASE;
  BootInfo.RamSize = 512ULL << 20;
  BootInfo.CpuCount = 1;
  FinalizeChecksum(&BootInfo);
  return BootInfo;
}

static int
Expect(const char *Name, uint32_t Actual, uint32_t Expected)
{
  if (Actual == Expected) {
    return 0;
  }
  fprintf(stderr, "%s: got %u expected %u\n", Name, Actual, Expected);
  return 1;
}

int
main(void)
{
  BRIDGE_VM_PC_BOOT_INFO BootInfo = ValidBootInfo();
  int Failures = 0;

  Failures += Expect("valid", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_SUCCESS);
  BootInfo.Magic = 0;
  Failures += Expect("magic", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_MAGIC);
  BootInfo = ValidBootInfo();
  BootInfo.AbiVersion = 2;
  Failures += Expect("shape", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_SHAPE);
  BootInfo = ValidBootInfo();
  BootInfo.Reserved5 = 1;
  Failures += Expect("reserved", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_SHAPE);
  BootInfo = ValidBootInfo();
  ++BootInfo.HeaderChecksum;
  Failures += Expect("checksum", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_CHECKSUM);
  BootInfo = ValidBootInfo();
  BootInfo.RsdpLength = BRIDGE_VM_PC_BOOT_INFO_IMAGE_SIZE;
  FinalizeChecksum(&BootInfo);
  Failures += Expect("tables", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_TABLE_RANGE);
  BootInfo = ValidBootInfo();
  BootInfo.CpuCount = BRIDGE_VM_PC_MAX_CPUS + 1;
  FinalizeChecksum(&BootInfo);
  Failures += Expect("machine", BridgeVmPcSecMain(&BootInfo), BRIDGE_VM_PC_SEC_BAD_MACHINE);
  return Failures != 0;
}
