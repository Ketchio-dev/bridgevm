// SPDX-License-Identifier: Apache-2.0
#ifndef BRIDGE_VM_PC_NVME_BLOCK_IO_H_
#define BRIDGE_VM_PC_NVME_BLOCK_IO_H_
#include <Library/BaseMemoryLib.h>
#include <Protocol/BlockIo.h>

#define BRIDGE_VM_PC_NVME_BLOCK_SIZE     512U
#define BRIDGE_VM_PC_NVME_LAST_BLOCK     2047ULL
#define BRIDGE_VM_PC_NVME_LBA0_MARKER    0x4D56454744495242ULL

STATIC
EFI_STATUS
BridgeVmPcValidateNvmeBlockIo (
  IN EFI_BOOT_SERVICES *BootServices,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_BLOCK_IO_PROTOCOL *BlockIo;
  EFI_HANDLE *Handles;
  EFI_STATUS Status;
  UINT8 *Block;
  UINT64 Marker;
  UINTN Count;

  Handles = NULL;
  Count = 0;
  Status = BootServices->LocateHandleBuffer (
                           ByProtocol,
                           &gEfiBlockIoProtocolGuid,
                           NULL,
                           &Count,
                           &Handles
                           );
  if (EFI_ERROR (Status) || (Count != 1)) {
    return EFI_NOT_FOUND;
  }
  Result->NvmeBlockIoCount = (UINT32)Count;
  BlockIo = NULL;
  Status = BootServices->HandleProtocol (
                           Handles[0],
                           &gEfiBlockIoProtocolGuid,
                           (VOID **)&BlockIo
                           );
  BootServices->FreePool (Handles);
  if (EFI_ERROR (Status) || (BlockIo == NULL) || (BlockIo->Media == NULL) ||
      !BlockIo->Media->MediaPresent ||
      (BlockIo->Media->BlockSize != BRIDGE_VM_PC_NVME_BLOCK_SIZE) ||
      (BlockIo->Media->LastBlock != BRIDGE_VM_PC_NVME_LAST_BLOCK))
  {
    return EFI_COMPROMISED_DATA;
  }
  Block = NULL;
  Status = BootServices->AllocatePool (
                           EfiBootServicesData,
                           BlockIo->Media->BlockSize,
                           (VOID **)&Block
                           );
  if (EFI_ERROR (Status)) {
    return Status;
  }
  if (Block == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  Status = BlockIo->ReadBlocks (
                      BlockIo,
                      BlockIo->Media->MediaId,
                      0,
                      BlockIo->Media->BlockSize,
                      Block
                      );
  Marker = 0;
  if (!EFI_ERROR (Status)) {
    CopyMem (&Marker, Block, sizeof (Marker));
  }
  BootServices->FreePool (Block);
  if (EFI_ERROR (Status) || (Marker != BRIDGE_VM_PC_NVME_LBA0_MARKER)) {
    return EFI_COMPROMISED_DATA;
  }
  Result->NvmeBlockSize = BlockIo->Media->BlockSize;
  Result->NvmeMediaPresent = 1;
  Result->NvmeReadCount = 1;
  Result->NvmeLastBlock = BlockIo->Media->LastBlock;
  Result->NvmeLba0Marker = Marker;
  return EFI_SUCCESS;
}

EFI_STATUS
BridgeVmPcValidatePcie (
  IN EFI_SYSTEM_TABLE *SystemTable,
  OUT volatile BRIDGE_VM_PC_PCIE_RESULT *Result
  )
{
  EFI_STATUS Status;

  if ((SystemTable == NULL) || (SystemTable->BootServices == NULL) || (Result == NULL)) {
    return EFI_INVALID_PARAMETER;
  }
  Result->FunctionCount = BRIDGE_VM_PC_PCIE_FUNCTION_COUNT;
  Status = BridgeVmPcDiscoverPci (SystemTable, Result);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Status = BridgeVmPcValidatePciIdentities (SystemTable, Result);
  if (EFI_ERROR (Status)) {
    return Status;
  }
  Status = BridgeVmPcValidateNvmeBar (SystemTable, Result);
  return EFI_ERROR (Status) ? Status : BridgeVmPcValidateNvmeBlockIo (SystemTable->BootServices, Result);
}
#endif
