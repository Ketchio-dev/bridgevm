/** @file
  Exercise standard UEFI variable services across a preserved vars backing.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include <Library/BaseMemoryLib.h>

#include "VariableProbe.h"

#define BRIDGE_VM_PC_VARIABLE_WRITTEN   1U
#define BRIDGE_VM_PC_VARIABLE_RESTORED  2U

STATIC EFI_GUID  mProofGuid = {
  0x4b7c7bbc, 0xf0f1, 0x45cc, { 0x87, 0xf1, 0x8a, 0xd1, 0x7b, 0x31, 0x25, 0x62 }
};
STATIC CONST CHAR16  mProofName[] = L"BridgeVmPcProof";
STATIC CONST UINT8   mProofPayload[16] = {
  0x42, 0x56, 0x4d, 0x56, 0x41, 0x52, 0x31, 0x00,
  0x5a, 0xc3, 0x17, 0xe1, 0x83, 0x6a, 0x9d, 0x24
};

EFI_STATUS
BridgeVmPcRunVariableProbe (
  IN EFI_SYSTEM_TABLE                *SystemTable,
  OUT BRIDGE_VM_PC_VARIABLE_RESULT  *Result
  )
{
  UINT8       Data[sizeof (mProofPayload)];
  UINTN       DataSize;
  UINT32      Attributes;
  UINT32      RequiredAttributes;
  EFI_STATUS  Status;

  if ((SystemTable == NULL) || (SystemTable->RuntimeServices == NULL) ||
      (Result == NULL))
  {
    return EFI_INVALID_PARAMETER;
  }
  if ((SystemTable->RuntimeServices->GetVariable == NULL) ||
      (SystemTable->RuntimeServices->SetVariable == NULL) ||
      (SystemTable->RuntimeServices->QueryVariableInfo == NULL))
  {
    return EFI_UNSUPPORTED;
  }

  RequiredAttributes = EFI_VARIABLE_NON_VOLATILE |
                       EFI_VARIABLE_BOOTSERVICE_ACCESS |
                       EFI_VARIABLE_RUNTIME_ACCESS;
  DataSize = sizeof (Data);
  Attributes = 0;
  Status = SystemTable->RuntimeServices->GetVariable (
                                           (CHAR16 *)mProofName,
                                           &mProofGuid,
                                           &Attributes,
                                           &DataSize,
                                           Data
                                           );
  if (Status == EFI_NOT_FOUND) {
    Status = SystemTable->RuntimeServices->SetVariable (
                                             (CHAR16 *)mProofName,
                                             &mProofGuid,
                                             RequiredAttributes,
                                             sizeof (mProofPayload),
                                             (VOID *)mProofPayload
                                             );
    if (EFI_ERROR (Status)) {
      return Status;
    }
    DataSize = sizeof (Data);
    Attributes = 0;
    Status = SystemTable->RuntimeServices->GetVariable (
                                             (CHAR16 *)mProofName,
                                             &mProofGuid,
                                             &Attributes,
                                             &DataSize,
                                             Data
                                             );
    Result->State = BRIDGE_VM_PC_VARIABLE_WRITTEN;
  } else {
    Result->State = BRIDGE_VM_PC_VARIABLE_RESTORED;
  }
  if (EFI_ERROR (Status) || (Attributes != RequiredAttributes) ||
      (DataSize != sizeof (mProofPayload)) ||
      (CompareMem (Data, mProofPayload, sizeof (mProofPayload)) != 0))
  {
    return EFI_COMPROMISED_DATA;
  }

  Status = SystemTable->RuntimeServices->QueryVariableInfo (
                                           RequiredAttributes,
                                           &Result->MaximumStorage,
                                           &Result->RemainingStorage,
                                           &Result->MaximumVariableSize
                                           );
  if (EFI_ERROR (Status) ||
      (Result->MaximumStorage < 0x1000) ||
      (Result->RemainingStorage > Result->MaximumStorage) ||
      (Result->MaximumVariableSize < sizeof (mProofPayload)))
  {
    return EFI_COMPROMISED_DATA;
  }
  Result->Attributes = Attributes;
  Result->GetVariable = (UINT64)(UINTN)SystemTable->RuntimeServices->GetVariable;
  Result->SetVariable = (UINT64)(UINTN)SystemTable->RuntimeServices->SetVariable;
  Result->QueryVariableInfo = (UINT64)(UINTN)SystemTable->RuntimeServices->QueryVariableInfo;
  return EFI_SUCCESS;
}
