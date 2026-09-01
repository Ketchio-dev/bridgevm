/** @file
  Fail-closed validation for the BridgeVM Virtual ARM PC ACPI v1 set.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include <IndustryStandard/Acpi65.h>
#include <Library/BaseLib.h>
#include <BridgeVmPc/BootInfo.h>
#include "PlatformTablesDxe.h"

#define READ_HEADER32(Table, Field) \
  ReadUnaligned32 ((CONST UINT32 *)((CONST UINT8 *)(Table) + \
                   OFFSET_OF (EFI_ACPI_DESCRIPTION_HEADER, Field)))
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
UINT8
AcpiTableBit (
  IN UINT32  Signature
  )
{
  switch (Signature) {
    case SIGNATURE_32 ('F', 'A', 'C', 'P'):
      return BIT0;
    case SIGNATURE_32 ('A', 'P', 'I', 'C'):
      return BIT1;
    case SIGNATURE_32 ('P', 'P', 'T', 'T'):
      return BIT2;
    case SIGNATURE_32 ('G', 'T', 'D', 'T'):
      return BIT3;
    case SIGNATURE_32 ('M', 'C', 'F', 'G'):
      return BIT4;
    case SIGNATURE_32 ('S', 'P', 'C', 'R'):
      return BIT5;
    case SIGNATURE_32 ('D', 'B', 'G', '2'):
      return BIT6;
    default:
      return 0;
  }
}
EFI_STATUS
BridgeVmPcValidateAcpi (
  IN CONST BRIDGE_VM_PC_BOOT_INFO  *BootInfo
  )
{
  CONST EFI_ACPI_6_5_ROOT_SYSTEM_DESCRIPTION_POINTER  *Rsdp;
  CONST EFI_ACPI_DESCRIPTION_HEADER                    *Xsdt;
  CONST EFI_ACPI_DESCRIPTION_HEADER                    *Table;
  CONST EFI_ACPI_DESCRIPTION_HEADER                    *Dsdt;
  CONST EFI_ACPI_6_5_FIXED_ACPI_DESCRIPTION_TABLE      *Fadt;
  UINT64                                               DsdtAddress;
  UINT64                                               TableAddress;
  UINTN                                                EntryCount;
  UINTN                                                Index;
  UINT32                                               TableLength;
  UINT32                                               TableSignature;
  UINT8                                                TableBit;
  UINT8                                                SeenTables;
  Rsdp = (CONST VOID *)(UINTN)BootInfo->RsdpGpa;
  if ((BootInfo->RsdpLength != sizeof (*Rsdp)) ||
      (Rsdp->Signature != EFI_ACPI_6_5_ROOT_SYSTEM_DESCRIPTION_POINTER_SIGNATURE) ||
      (Rsdp->Revision < EFI_ACPI_6_5_ROOT_SYSTEM_DESCRIPTION_POINTER_REVISION) ||
      (Rsdp->Length != sizeof (*Rsdp)) ||
      !ChecksumIsZero (Rsdp, 20) ||
      !ChecksumIsZero (Rsdp, sizeof (*Rsdp)) ||
      (Rsdp->XsdtAddress != BootInfo->AcpiTablesGpa))
  {
    return EFI_COMPROMISED_DATA;
  }
  Xsdt           = (CONST VOID *)(UINTN)BootInfo->AcpiTablesGpa;
  TableLength    = READ_HEADER32 (Xsdt, Length);
  TableSignature = READ_HEADER32 (Xsdt, Signature);
  if ((TableSignature != EFI_ACPI_6_5_EXTENDED_SYSTEM_DESCRIPTION_TABLE_SIGNATURE) ||
      (TableLength < sizeof (*Xsdt)) ||
      (TableLength > BootInfo->AcpiTablesLength) ||
      (((TableLength - sizeof (*Xsdt)) % sizeof (UINT64)) != 0) ||
      !ChecksumIsZero (Xsdt, TableLength))
  {
    return EFI_COMPROMISED_DATA;
  }
  EntryCount = (TableLength - sizeof (*Xsdt)) / sizeof (UINT64);
  if (EntryCount != 7) {
    return EFI_COMPROMISED_DATA;
  }
  Fadt       = NULL;
  SeenTables = 0;
  for (Index = 0; Index < EntryCount; Index++) {
    TableAddress = ReadUnaligned64 (
                     (CONST UINT64 *)((CONST UINT8 *)Xsdt + sizeof (*Xsdt) +
                                      (Index * sizeof (UINT64)))
                     );
    if (!RangeWithin (
           TableAddress,
           sizeof (*Table),
           BootInfo->AcpiTablesGpa,
           BootInfo->AcpiTablesLength
           ))
    {
      return EFI_COMPROMISED_DATA;
    }

    Table          = (CONST VOID *)(UINTN)TableAddress;
    TableLength    = READ_HEADER32 (Table, Length);
    TableSignature = READ_HEADER32 (Table, Signature);
    if ((TableLength < sizeof (*Table)) ||
        !RangeWithin (
           TableAddress,
           TableLength,
           BootInfo->AcpiTablesGpa,
           BootInfo->AcpiTablesLength
           ) ||
        !ChecksumIsZero (Table, TableLength))
    {
      return EFI_COMPROMISED_DATA;
    }

    TableBit = AcpiTableBit (TableSignature);
    if ((TableBit == 0) || ((SeenTables & TableBit) != 0)) {
      return EFI_COMPROMISED_DATA;
    }

    SeenTables = (UINT8)(SeenTables | TableBit);
    if (TableSignature == EFI_ACPI_6_5_FIXED_ACPI_DESCRIPTION_TABLE_SIGNATURE) {
      if (TableLength != sizeof (*Fadt)) {
        return EFI_COMPROMISED_DATA;
      }

      Fadt = (CONST VOID *)Table;
    }
  }

  if ((SeenTables != 0x7F) || (Fadt == NULL)) {
    return EFI_COMPROMISED_DATA;
  }

  DsdtAddress = ReadUnaligned64 (
                  (CONST UINT64 *)((CONST UINT8 *)Fadt +
                                   OFFSET_OF (
                                     EFI_ACPI_6_5_FIXED_ACPI_DESCRIPTION_TABLE,
                                     XDsdt
                                     ))
                  );
  if (!RangeWithin (
         DsdtAddress,
         sizeof (*Dsdt),
         BootInfo->AcpiTablesGpa,
         BootInfo->AcpiTablesLength
         ))
  {
    return EFI_COMPROMISED_DATA;
  }

  Dsdt           = (CONST VOID *)(UINTN)DsdtAddress;
  TableLength    = READ_HEADER32 (Dsdt, Length);
  TableSignature = READ_HEADER32 (Dsdt, Signature);
  if ((TableSignature != SIGNATURE_32 ('D', 'S', 'D', 'T')) ||
      (TableLength < sizeof (*Dsdt)) ||
      !RangeWithin (
         DsdtAddress,
         TableLength,
         BootInfo->AcpiTablesGpa,
         BootInfo->AcpiTablesLength
         ) ||
      !ChecksumIsZero (Dsdt, TableLength))
  {
    return EFI_COMPROMISED_DATA;
  }

  return EFI_SUCCESS;
}
