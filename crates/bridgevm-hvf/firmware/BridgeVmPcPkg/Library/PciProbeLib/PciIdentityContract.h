// Modeled host bridge, NVMe and xHCI only. SPDX-License-Identifier: Apache-2.0
STATIC CONST UINT32 mExpectedIdentity[BRIDGE_VM_PC_PCIE_FUNCTION_COUNT] = {
  0x00081B36U, 0x00101B36U, 0x000D1B36U
};
STATIC UINT32
BridgeVmPcIdentityLocationValid (
  UINTN Segment, UINTN Bus, UINTN Device, UINTN Function, UINT32 Seen
  )
{
  return (Segment == 0) && (Bus == 0) &&
         (Device < BRIDGE_VM_PC_PCIE_FUNCTION_COUNT) && (Function == 0) &&
         ((Seen & (1U << Device)) == 0);
}
STATIC VOID
BridgeVmPcClearIdentitySlots (volatile BRIDGE_VM_PC_PCIE_RESULT *Result)
{
  UINTN Index;
  for (Index = 0; Index < BRIDGE_VM_PC_PCIE_IDENTITY_CAPACITY; ++Index) {
    Result->Identity[Index] = 0;
  }
}
