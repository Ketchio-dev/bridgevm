#!/usr/bin/env bash
# Deterministic source/provenance guard for the independent firmware package.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
HEADER="$PKG/Include/BridgeVmPc/BootInfo.h"
RUST_INFO="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc_boot_info.rs"
RUST_BOARD="$ROOT/crates/bridgevm-hvf/src/bridgevm_pc.rs"
if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m' "$PKG"; then
  echo "FAIL: BridgeVmPcPkg references a prohibited compatibility platform" >&2
  exit 1
fi
if find "$PKG" -type f ! \( -name '*.c' -o -name '*.h' -o -name '*.dec' -o -name '*.dsc' -o -name '*.inf' -o -name '*.S' -o -name '*.ld' \) | grep -q .; then
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
  "$PKG"/Drivers/*/*.inf | sort -u |
  diff -u - <(printf '%s\n' BridgeVmPcPkg/BridgeVmPcPkg.dec MdePkg/MdePkg.dec)
grep -Fq 'b03a21a63e3bd001f52c527e5a57feddb53a690b' "$ROOT/scripts/build-bridgevm-pc-dxe-core-fv.sh"
grep -Fq 'MdeModulePkg/Core/Dxe/DxeMain.inf' "$ROOT/scripts/build-bridgevm-pc-dxe-core-fv.sh"
grep -Fq '022e09f7e60c3f1cf5b1416a66714b642714e827ba085957383ea3264f3f4ed6' "$ROOT/scripts/build-bridgevm-pc-dxe-core-fv.sh"
grep -Fq 'BridgeVmPcDxeProbe.efi' "$ROOT/scripts/build-bridgevm-pc-dxe-entry-firmware.sh"
bash -n "$ROOT/scripts/build-bridgevm-pc-edk2-consumer.sh" "$ROOT/scripts/build-bridgevm-pc-dxe-core-fv.sh" "$ROOT/scripts/build-bridgevm-pc-dxe-entry-firmware.sh"
"$ROOT/scripts/check-bridgevm-pc-reset-vector-boundary.sh"
echo "PASS: BridgeVmPcPkg uses only its versioned ABI and approved generic EDK2 boundary"
