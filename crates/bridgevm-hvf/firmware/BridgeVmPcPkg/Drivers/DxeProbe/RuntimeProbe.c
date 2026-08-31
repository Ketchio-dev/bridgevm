// RuntimeDxe service probe. SPDX-License-Identifier: Apache-2.0
#include <Uefi.h>
#include <Protocol/Runtime.h>
#include "PcieProbe.h"
#include "Result.h"
#include "RuntimeProbe.h"
#include "VariableProbe.h"
#define BRIDGE_VM_PC_DXE_RESULT_GPA  0x100002000ULL
#define BRIDGE_VM_PC_VARIABLE_WRITTEN_STAGE   10U
#define BRIDGE_VM_PC_VARIABLE_RESTORED_STAGE  11U
#define BRIDGE_VM_PC_RUNTIME_CRC32  0x3f6f728dU
STATIC CONST CHAR8  mRuntimeCrcPayload[] = "BridgeVM RuntimeDxe v1";
EFI_STATUS
BridgeVmPcRunRuntimeProbe (
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  volatile BRIDGE_VM_PC_DXE_RESULT  *Result;
  EFI_RUNTIME_ARCH_PROTOCOL  *RuntimeProtocol;
  EFI_STATUS                 Status;
  BRIDGE_VM_PC_VARIABLE_RESULT  Variable;
  UINT32                     Crc32;
  if ((SystemTable == NULL) || (SystemTable->BootServices == NULL) ||
      (SystemTable->RuntimeServices == NULL))
  {
    return EFI_INVALID_PARAMETER;
  }
  if ((SystemTable->RuntimeServices->Hdr.Signature != EFI_RUNTIME_SERVICES_SIGNATURE) ||
      (SystemTable->RuntimeServices->SetVirtualAddressMap == NULL) ||
      (SystemTable->RuntimeServices->ConvertPointer == NULL) ||
      (SystemTable->BootServices->CalculateCrc32 == NULL))
  {
    return EFI_COMPROMISED_DATA;
  }
  RuntimeProtocol = NULL;
  Status = SystemTable->BootServices->LocateProtocol (
                                        &gEfiRuntimeArchProtocolGuid,
                                        NULL,
                                        (VOID **)&RuntimeProtocol
                                        );
  if (EFI_ERROR (Status) || (RuntimeProtocol == NULL)) {
    return EFI_NOT_FOUND;
  }
  Status = SystemTable->BootServices->CalculateCrc32 (
                                        (VOID *)mRuntimeCrcPayload,
                                        sizeof (mRuntimeCrcPayload) - 1,
                                        &Crc32
                                        );
  if (EFI_ERROR (Status) || (Crc32 != BRIDGE_VM_PC_RUNTIME_CRC32)) {
    return EFI_COMPROMISED_DATA;
  }
  Status = BridgeVmPcRunVariableProbe (SystemTable, &Variable);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Result = (volatile BRIDGE_VM_PC_DXE_RESULT *)(UINTN)BRIDGE_VM_PC_DXE_RESULT_GPA;
  Status = BridgeVmPcValidatePcieIdentities (Result->PcieIdentity);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Result->RuntimeCrc32 = Crc32;
  Result->SystemTable = (UINT64)(UINTN)SystemTable;
  Result->RuntimeServices = (UINT64)(UINTN)SystemTable->RuntimeServices;
  Result->RuntimeProtocol = (UINT64)(UINTN)RuntimeProtocol;
  Result->SetVirtualAddressMap = (UINT64)(UINTN)SystemTable->RuntimeServices->SetVirtualAddressMap;
  Result->ConvertPointer = (UINT64)(UINTN)SystemTable->RuntimeServices->ConvertPointer;
  Result->CalculateCrc32 = (UINT64)(UINTN)SystemTable->BootServices->CalculateCrc32;
  Result->Variable = Variable;
  Result->PcieFunctionCount = BRIDGE_VM_PC_PCIE_FUNCTION_COUNT;
  Result->Stage = (Variable.State == 1) ?
                  BRIDGE_VM_PC_VARIABLE_WRITTEN_STAGE :
                  BRIDGE_VM_PC_VARIABLE_RESTORED_STAGE;
  __asm__ __volatile__("dsb sy\n\thvc #0" ::: "memory");
  return EFI_DEVICE_ERROR;
}
