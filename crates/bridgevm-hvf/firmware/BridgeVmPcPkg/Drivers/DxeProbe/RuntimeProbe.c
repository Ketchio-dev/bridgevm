/** @file
  Prove generic RuntimeDxe services without claiming a complete UEFI platform.

  SPDX-License-Identifier: Apache-2.0
**/

#include <Uefi.h>
#include <Protocol/Runtime.h>

#include "RuntimeProbe.h"

#define BRIDGE_VM_PC_DXE_RESULT_GPA  0x100002000ULL
#define BRIDGE_VM_PC_DXE_DISPATCHED  9U
#define BRIDGE_VM_PC_RUNTIME_CRC32  0x3f6f728dU

STATIC CONST CHAR8  mRuntimeCrcPayload[] = "BridgeVM RuntimeDxe v1";

typedef struct {
  UINT32 Stage;
  UINT32 RuntimeCrc32;
  UINT64 SystemTable;
  UINT64 RuntimeServices;
  UINT64 RuntimeProtocol;
  UINT64 SetVirtualAddressMap;
  UINT64 ConvertPointer;
  UINT64 CalculateCrc32;
} BRIDGE_VM_PC_DXE_RESULT;

EFI_STATUS
BridgeVmPcRunRuntimeProbe (
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  volatile BRIDGE_VM_PC_DXE_RESULT  *Result;
  EFI_RUNTIME_ARCH_PROTOCOL  *RuntimeProtocol;
  EFI_STATUS                 Status;
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
  Result = (volatile BRIDGE_VM_PC_DXE_RESULT *)(UINTN)BRIDGE_VM_PC_DXE_RESULT_GPA;
  Result->RuntimeCrc32 = Crc32;
  Result->SystemTable = (UINT64)(UINTN)SystemTable;
  Result->RuntimeServices = (UINT64)(UINTN)SystemTable->RuntimeServices;
  Result->RuntimeProtocol = (UINT64)(UINTN)RuntimeProtocol;
  Result->SetVirtualAddressMap = (UINT64)(UINTN)SystemTable->RuntimeServices->SetVirtualAddressMap;
  Result->ConvertPointer = (UINT64)(UINTN)SystemTable->RuntimeServices->ConvertPointer;
  Result->CalculateCrc32 = (UINT64)(UINTN)SystemTable->BootServices->CalculateCrc32;
  Result->Stage = BRIDGE_VM_PC_DXE_DISPATCHED;
  __asm__ __volatile__("dsb sy\n\thvc #0" ::: "memory");
  return EFI_DEVICE_ERROR;
}
