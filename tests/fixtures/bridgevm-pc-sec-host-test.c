/* SPDX-License-Identifier: Apache-2.0 */
/* Deterministic host tests for the freestanding BridgeVM PC SEC validator. */
#include <stdio.h>
#include "BridgeVmPcHob.h"

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
Expect(const char *Name, uint64_t Actual, uint64_t Expected)
{
  if (Actual == Expected) {
    return 0;
  }
  fprintf(stderr, "%s: got %llu expected %llu\n", Name, (unsigned long long)Actual, (unsigned long long)Expected);
  return 1;
}

int
main(void)
{
  BRIDGE_VM_PC_BOOT_INFO BootInfo = ValidBootInfo();
  BRIDGE_VM_PC_SEC_RESULT Result = {0};
  uint8_t Hob[BRIDGE_VM_PC_HOB_LIST_SIZE] = {0};
  BRIDGE_VM_PC_HOB_HANDOFF_INFO *Phit = (void *)Hob; BRIDGE_VM_PC_HOB_RESOURCE *Resource = (void *)(Hob + 56);
  BRIDGE_VM_PC_HOB_STACK *Stack = (void *)(Hob + 104); BRIDGE_VM_PC_HOB_CPU_INFO *Cpu = (void *)(Hob + 152); BRIDGE_VM_PC_HOB_HEADER *End = (void *)(Hob + 168);
  int Failures = 0;

  Failures += Expect("valid", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 1); Failures += Expect("result stage/count", Result.Stage << 16 | Result.HobCount, 0x00010005);
  Failures += Expect("result GPA", Result.HobListGpa, BRIDGE_VM_PC_HOB_LIST_GPA); Failures += Expect("result size", Result.HobListSize, sizeof(Hob));
  Failures += Expect("PHIT header", Phit->Header.HobType << 16 | Phit->Header.HobLength, 0x00010038); Failures += Expect("PHIT version", Phit->Version, 9);
  Failures += Expect("PHIT memory top", Phit->EfiMemoryTop, BRIDGE_VM_PC_RAM_BASE + BootInfo.RamSize); Failures += Expect("PHIT free bottom", Phit->EfiFreeMemoryBottom, BRIDGE_VM_PC_FREE_MEMORY_BOTTOM);
  Failures += Expect("PHIT end", Phit->EfiEndOfHobList, BRIDGE_VM_PC_HOB_LIST_GPA + 168); Failures += Expect("resource header", Resource->Header.HobType << 16 | Resource->Header.HobLength, 0x00030030);
  Failures += Expect("resource attributes", Resource->ResourceAttribute, 0x2007); Failures += Expect("resource length", Resource->ResourceLength, BootInfo.RamSize);
  Failures += Expect("stack header", Stack->Header.HobType << 16 | Stack->Header.HobLength, 0x00020030); Failures += Expect("stack base", Stack->Allocation.MemoryBaseAddress, BRIDGE_VM_PC_STACK_BASE);
  Failures += Expect("stack size", Stack->Allocation.MemoryLength, BRIDGE_VM_PC_STACK_SIZE); Failures += Expect("CPU header", Cpu->Header.HobType << 16 | Cpu->Header.HobLength, 0x00060010);
  Failures += Expect("CPU PA bits", Cpu->SizeOfMemorySpace, 40); Failures += Expect("end header", (uint32_t)End->HobType << 16 | End->HobLength, 0xFFFF0008);
  BootInfo.Magic = 0; Failures += Expect("magic", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 2);
  BootInfo = ValidBootInfo(); BootInfo.AbiVersion = 2; Failures += Expect("shape", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 3);
  BootInfo = ValidBootInfo(); BootInfo.Reserved5 = 1; Failures += Expect("reserved", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 3);
  BootInfo = ValidBootInfo(); ++BootInfo.HeaderChecksum; Failures += Expect("checksum", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 4);
  BootInfo = ValidBootInfo(); BootInfo.RsdpLength = BRIDGE_VM_PC_BOOT_INFO_IMAGE_SIZE; FinalizeChecksum(&BootInfo); Failures += Expect("tables", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 5);
  BootInfo = ValidBootInfo(); BootInfo.CpuCount = BRIDGE_VM_PC_MAX_CPUS + 1; FinalizeChecksum(&BootInfo); Failures += Expect("machine", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 6);
  BootInfo = ValidBootInfo(); BootInfo.RamSize = BRIDGE_VM_PC_FREE_MEMORY_BOTTOM - BRIDGE_VM_PC_RAM_BASE - 1; FinalizeChecksum(&BootInfo); Failures += Expect("small RAM", BridgeVmPcSecMain(&BootInfo, &Result, Hob), 6);
  Failures += Expect("null HOB", BridgeVmPcSecMain(&BootInfo, &Result, 0), 7);
  return Failures != 0;
}
