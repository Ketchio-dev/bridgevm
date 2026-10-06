// Exercise the actual firmware identity validator with controlled UEFI providers.
#include <stdio.h>
#include <string.h>
#include BRIDGEVM_PCI_IDENTITIES_SOURCE
_Static_assert(sizeof(BRIDGE_VM_PC_PCIE_RESULT) == 128, "result ABI size changed");
_Static_assert(offsetof(BRIDGE_VM_PC_PCIE_RESULT, RootBridgeCount) == 36, "root ABI changed");
_Static_assert(offsetof(BRIDGE_VM_PC_PCIE_RESULT, NvmeBarReadCount) == 56, "NVMe BAR ABI changed");
_Static_assert(offsetof(BRIDGE_VM_PC_PCIE_RESULT, NvmeBlockIoCount) == 96, "Block I/O ABI changed");

EFI_GUID gEfiPciIoProtocolGuid = { 0 };
static EFI_HANDLE handles[8];
static EFI_PCI_IO_PROTOCOL protocols[8];
static UINTN count, devices[8], buses[8], functions[8], segments[8];
static UINT32 identities[8];
static int provider_failure, failures, releases;

static EFI_STATUS locate(int kind, EFI_GUID *guid, VOID *key, UINTN *n, EFI_HANDLE **out) {
  (void)kind; (void)guid; (void)key;
  *n = count; *out = handles;
  return provider_failure == 1 ? EFI_DEVICE_ERROR : EFI_SUCCESS;
}
static EFI_STATUS protocol(EFI_HANDLE handle, EFI_GUID *guid, VOID **out) {
  (void)guid; *out = handle;
  return provider_failure == 2 ? EFI_DEVICE_ERROR : EFI_SUCCESS;
}
static EFI_STATUS release(VOID *allocation) {
  if (allocation != handles) return EFI_DEVICE_ERROR;
  releases++; return EFI_SUCCESS;
}
static EFI_STATUS location(EFI_PCI_IO_PROTOCOL *p, UINTN *s, UINTN *b, UINTN *d, UINTN *f) {
  UINTN i = (UINTN)(p - protocols);
  *s = segments[i]; *b = buses[i]; *d = devices[i]; *f = functions[i];
  return provider_failure == 3 ? EFI_DEVICE_ERROR : EFI_SUCCESS;
}
static EFI_STATUS read_identity(EFI_PCI_IO_PROTOCOL *p, int width, UINT32 offset, UINTN n, VOID *out) {
  if (width != EfiPciIoWidthUint32 || offset != 0 || n != 1) return EFI_DEVICE_ERROR;
  *(UINT32 *)out = identities[p - protocols];
  return provider_failure == 4 ? EFI_DEVICE_ERROR : EFI_SUCCESS;
}
static void reset(void) {
  static const UINT32 modeled[] = {0x00081b36, 0x00101b36, 0x000d1b36};
  count = 3; provider_failure = 0; releases = 0;
  memset(buses, 0, sizeof buses); memset(functions, 0, sizeof functions);
  memset(segments, 0, sizeof segments);
  for (UINTN i = 0; i < 8; ++i) {
    devices[i] = i; identities[i] = i < 3 ? modeled[i] : 0;
    protocols[i].GetLocation = location; protocols[i].Pci.Read = read_identity;
    handles[i] = &protocols[i];
  }
}
static void check(const char *name, int success) {
  EFI_BOOT_SERVICES services = {locate, protocol, release};
  EFI_SYSTEM_TABLE table = {&services};
  BRIDGE_VM_PC_PCIE_RESULT result;
  memset(&result, 0xa5, sizeof result);
  EFI_STATUS status = BridgeVmPcValidatePciIdentities(&table, &result);
  int passed = ((status == EFI_SUCCESS) == success) && releases == (provider_failure == 1 ? 0 : 1);
  if (success && status == EFI_SUCCESS) {
    for (UINTN i = 0; i < 3; ++i) passed &= result.Identity[i] == identities[i];
    for (UINTN i = 3; i < 8; ++i) passed &= result.Identity[i] == 0;
  }
  printf("%s: %s (status=%llx)\n", name, passed ? "PASS" : "FAIL", (unsigned long long)status);
  failures += !passed;
}
int main(void) {
  reset(); check("exact modeled identities", 1);
  reset(); handles[0] = &protocols[2]; handles[2] = &protocols[0]; check("reordered handles", 1);
  reset(); count = 2; check("missing endpoint", 0);
  reset(); count = 4; check("extra endpoint", 0);
  reset(); count = 8; check("historical ghost endpoints", 0);
  reset(); devices[2] = 1; check("duplicate BDF", 0);
  reset(); devices[2] = 7; check("unmodeled BDF", 0);
  reset(); identities[1] ^= 1; check("wrong NVMe identity", 0);
  reset(); identities[2] ^= 1; check("wrong xHCI identity", 0);
  reset(); buses[2] = 1; check("wrong bus", 0);
  reset(); functions[2] = 1; check("wrong function", 0);
  reset(); segments[2] = 1; check("wrong segment", 0);
  for (int i = 1; i <= 4; ++i) {
    reset(); provider_failure = i; check("provider error", 0);
  }
  return failures ? 1 : 0;
}
