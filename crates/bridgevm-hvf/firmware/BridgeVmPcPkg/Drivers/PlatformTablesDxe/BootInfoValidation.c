/** @file
  Fail-closed validation for BridgeVM boot-info v1.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include <IndustryStandard/SmBios.h>
#include <Library/BaseMemoryLib.h>
#include <BridgeVmPc/BootInfo.h>

#include "PlatformTablesDxe.h"

STATIC
BOOLEAN
ChecksumIsZero (
  IN CONST VOID  *Buffer,
  IN UINTN       Length
  )
{
  CONST UINT8  *Bytes;
  UINT8        Sum;
  UINTN        Index;

  Bytes = Buffer;
  Sum   = 0;
  for (Index = 0; Index < Length; Index++) {
    Sum = (UINT8)(Sum + Bytes[Index]);
  }

  return Sum == 0;
}

STATIC
BOOLEAN
RangeWithin (
  IN UINT64  Address,
  IN UINT64  Length,
  IN UINT64  Base,
  IN UINT64  Size
  )
{
  UINT64  Offset;

  if ((Length == 0) || (Address < Base)) {
    return FALSE;
  }

  Offset = Address - Base;
  return (Offset <= Size) && (Length <= (Size - Offset));
}

STATIC
EFI_STATUS
ValidateSmbios (
  IN CONST BRIDGE_VM_PC_BOOT_INFO  *BootInfo
  )
{
  CONST SMBIOS_TABLE_3_0_ENTRY_POINT  *Smbios;
  CONST SMBIOS_STRUCTURE                *Structure;
  CONST UINT8                           *Cursor;
  CONST UINT8                           *End;
  CONST UINT8                           *Strings;
  BOOLEAN                               FoundEnd;

  Smbios = (CONST VOID *)(UINTN)BootInfo->SmbiosAnchorGpa;
  if ((BootInfo->SmbiosAnchorLength != sizeof (*Smbios)) ||
      (CompareMem (
         Smbios->AnchorString,
         SMBIOS_3_0_ANCHOR_STRING,
         SMBIOS_3_0_ANCHOR_STRING_LENGTH
         ) != 0) ||
      (Smbios->EntryPointLength != sizeof (*Smbios)) ||
      (Smbios->MajorVersion != 3) || (Smbios->MinorVersion != 0) ||
      (Smbios->DocRev != 0) || (Smbios->EntryPointRevision != 1) ||
      (Smbios->Reserved != 0) ||
      (Smbios->TableMaximumSize != BootInfo->SmbiosTablesLength) ||
      (Smbios->TableAddress != BootInfo->SmbiosTablesGpa) ||
      !ChecksumIsZero (Smbios, sizeof (*Smbios)))
  {
    return EFI_COMPROMISED_DATA;
  }

  Cursor   = (CONST VOID *)(UINTN)BootInfo->SmbiosTablesGpa;
  End      = Cursor + BootInfo->SmbiosTablesLength;
  FoundEnd = FALSE;
  while (Cursor < End) {
    if ((UINTN)(End - Cursor) < sizeof (*Structure)) {
      return EFI_COMPROMISED_DATA;
    }

    Structure = (CONST VOID *)Cursor;
    if ((Structure->Length < sizeof (*Structure)) ||
        (Structure->Length > (UINTN)(End - Cursor)))
    {
      return EFI_COMPROMISED_DATA;
    }

    Strings = Cursor + Structure->Length;
    while (((UINTN)(End - Strings) >= 2) &&
           ((Strings[0] != 0) || (Strings[1] != 0)))
    {
      Strings++;
    }

    if ((UINTN)(End - Strings) < 2) {
      return EFI_COMPROMISED_DATA;
    }

    Cursor = Strings + 2;
    if (Structure->Type == SMBIOS_TYPE_END_OF_TABLE) {
      FoundEnd = TRUE;
      break;
    }
  }

  return (FoundEnd && (Cursor == End)) ? EFI_SUCCESS : EFI_COMPROMISED_DATA;
}

EFI_STATUS
BridgeVmPcValidateBootInfo (
  OUT CONST BRIDGE_VM_PC_BOOT_INFO  **BootInfo
  )
{
  CONST BRIDGE_VM_PC_BOOT_INFO  *Candidate;
  EFI_STATUS                    Status;

  if (BootInfo == NULL) {
    return EFI_INVALID_PARAMETER;
  }

  Candidate = (CONST VOID *)(UINTN)BRIDGE_VM_PC_BOOT_INFO_BASE;
  if ((Candidate->Magic != BRIDGE_VM_PC_BOOT_INFO_MAGIC) ||
      (Candidate->AbiVersion != BRIDGE_VM_PC_BOOT_INFO_ABI) ||
      (Candidate->HeaderSize != BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE) ||
      (Candidate->ImageSize != BRIDGE_VM_PC_BOOT_INFO_SIZE) ||
      (Candidate->Flags != BRIDGE_VM_PC_BOOT_INFO_VALID) ||
      (Candidate->Reserved0 != 0) || (Candidate->Reserved1 != 0) ||
      (Candidate->Reserved2 != 0) || (Candidate->Reserved3 != 0) ||
      (Candidate->Reserved4 != 0) || (Candidate->Reserved5 != 0) ||
      !ChecksumIsZero (Candidate, Candidate->HeaderSize) ||
      (Candidate->RsdpGpa != BRIDGE_VM_PC_BOOT_INFO_RSDP) ||
      (Candidate->AcpiTablesGpa != BRIDGE_VM_PC_BOOT_INFO_ACPI) ||
      (Candidate->SmbiosAnchorGpa != BRIDGE_VM_PC_BOOT_INFO_SMBIOS_ANCHOR) ||
      (Candidate->SmbiosTablesGpa != BRIDGE_VM_PC_BOOT_INFO_SMBIOS_TABLES) ||
      (Candidate->RamBase != BRIDGE_VM_PC_RAM_BASE) ||
      (Candidate->RamSize == 0) ||
      (Candidate->CpuCount == 0) ||
      (Candidate->CpuCount > BRIDGE_VM_PC_MAX_CPUS) ||
      !RangeWithin (
         Candidate->RsdpGpa,
         Candidate->RsdpLength,
         BRIDGE_VM_PC_BOOT_INFO_BASE,
         BRIDGE_VM_PC_BOOT_INFO_SIZE
         ) ||
      !RangeWithin (
         Candidate->AcpiTablesGpa,
         Candidate->AcpiTablesLength,
         BRIDGE_VM_PC_BOOT_INFO_BASE,
         BRIDGE_VM_PC_BOOT_INFO_SIZE
         ) ||
      !RangeWithin (
         Candidate->SmbiosAnchorGpa,
         Candidate->SmbiosAnchorLength,
         BRIDGE_VM_PC_BOOT_INFO_BASE,
         BRIDGE_VM_PC_BOOT_INFO_SIZE
         ) ||
      !RangeWithin (
         Candidate->SmbiosTablesGpa,
         Candidate->SmbiosTablesLength,
         BRIDGE_VM_PC_BOOT_INFO_BASE,
         BRIDGE_VM_PC_BOOT_INFO_SIZE
         ))
  {
    return EFI_COMPROMISED_DATA;
  }

  Status = BridgeVmPcValidateAcpi (Candidate);
  if (EFI_ERROR (Status)) {
    return Status;
  }

  Status = ValidateSmbios (Candidate);
  if (EFI_ERROR (Status)) {
    return Status;
  }

  *BootInfo = Candidate;
  return EFI_SUCCESS;
}
