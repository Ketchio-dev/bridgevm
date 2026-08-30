#!/usr/bin/env bash
# Deterministic source/provenance guard for the independent firmware package.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
HEADER="$PKG/Include/BridgeVmPc/BootInfo.h"
RUST_INFO="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc_boot_info.rs"
RUST_BOARD="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc.rs"

if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|utm' "$PKG"; then
  echo "FAIL: BridgeVmPcPkg references a prohibited compatibility platform" >&2
  exit 1
fi
if find "$PKG" -type f ! \( -name '*.c' -o -name '*.h' -o -name '*.dec' -o -name '*.dsc' -o -name '*.inf' \) | grep -q .; then
  echo "FAIL: BridgeVmPcPkg contains an unapproved file type" >&2
  exit 1
fi

required_header_values=(
  '#define BRIDGE_VM_PC_BOOT_INFO_BASE           0x26000000ULL'
  '#define BRIDGE_VM_PC_BOOT_INFO_SIZE           0x00010000U'
  '#define BRIDGE_VM_PC_BOOT_INFO_ABI            1U'
  '#define BRIDGE_VM_PC_BOOT_INFO_HEADER_SIZE    112U'
  '#define BRIDGE_VM_PC_BOOT_INFO_RSDP           0x26001000ULL'
  '#define BRIDGE_VM_PC_BOOT_INFO_ACPI           0x26002000ULL'
  '#define BRIDGE_VM_PC_BOOT_INFO_SMBIOS_ANCHOR  0x2600C000ULL'
  '#define BRIDGE_VM_PC_BOOT_INFO_SMBIOS_TABLES  0x2600D000ULL'
  '#define BRIDGE_VM_PC_RAM_BASE                 0x100000000ULL'
  '#define BRIDGE_VM_PC_MAX_CPUS                 64U'
)
for value in "${required_header_values[@]}"; do
  grep -Fqx "$value" "$HEADER" || { echo "FAIL: missing ABI value: $value" >&2; exit 1; }
done

grep -Fq 'pub const BOOT_INFO_HEADER_SIZE: usize = 112;' "$RUST_INFO"
grep -Fq 'pub const BOOT_INFO_RSDP_OFFSET: usize = 0x1000;' "$RUST_INFO"
grep -Fq 'pub const BOOT_INFO_ACPI_OFFSET: usize = 0x2000;' "$RUST_INFO"
grep -Fq 'pub const BOOT_INFO_SMBIOS_ANCHOR_OFFSET: usize = 0xc000;' "$RUST_INFO"
grep -Fq 'pub const BOOT_INFO_SMBIOS_TABLES_OFFSET: usize = 0xd000;' "$RUST_INFO"
grep -Fq 'pub const BOARD_ABI_VERSION: u32 = 1;' "$RUST_BOARD"
grep -Fq 'pub const MAX_CPUS: u64 = 64;' "$RUST_BOARD"
grep -Fq 'pub const RAM_BASE: u64 = 0x1_0000_0000;' "$RUST_BOARD"
grep -Fq 'pub const BOOT_INFO: Region = Region::new(0x2600_0000, 0x1_0000);' "$RUST_BOARD"

awk '/^[[:space:]]+[A-Za-z0-9]+Pkg\/.*\.dec$/ {print $1}' \
  "$PKG/Drivers/PlatformTablesDxe/PlatformTablesDxe.inf" |
  diff -u - <(printf '%s\n' MdePkg/MdePkg.dec BridgeVmPcPkg/BridgeVmPcPkg.dec)
bash -n "$ROOT/scripts/build-bridgevm-pc-edk2-consumer.sh"
echo "PASS: BridgeVmPcPkg uses only its versioned ABI and approved generic EDK2 boundary"
