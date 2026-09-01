//! Minimal SMBIOS blobs for the Path A `fw_cfg` handoff.
//!
//! The current firmware reads `etc/smbios/smbios-tables` and installs the
//! records through EFI's SMBIOS protocol. Entry-point address and checksum
//! fields remain zero until firmware places and finalizes the tables.

use crate::machine::{self, bridgevm_pc};

/// fw_cfg file carrying the SMBIOS 3.0 entry point.
pub const SMBIOS_ANCHOR_FILE: &str = "etc/smbios/smbios-anchor";
/// fw_cfg file carrying concatenated SMBIOS structures.
pub const SMBIOS_TABLE_FILE: &str = "etc/smbios/smbios-tables";

const HANDLE_TYPE0: u16 = 0x0000;
const HANDLE_TYPE1: u16 = 0x0100;
const HANDLE_TYPE3: u16 = 0x0300;
const HANDLE_TYPE4: u16 = 0x0400;
const HANDLE_TYPE16: u16 = 0x1000;
const HANDLE_TYPE17: u16 = 0x1100;
const HANDLE_TYPE19: u16 = 0x1300;
const HANDLE_TYPE32: u16 = 0x2000;
const HANDLE_TYPE127: u16 = 0x7F00;

const KB: u64 = 1024;
const MB: u64 = 1024 * 1024;
const MAX_TYPE16_STD_KB: u64 = 0x8000_0000;
const MAX_TYPE17_STD_MB: u64 = 0x7FFF;

/// The two SMBIOS blobs registered in fw_cfg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbiosBlobs {
    pub anchor: Vec<u8>,
    pub tables: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SmbiosPlatform {
    firmware_vendor: &'static str,
    firmware_version: &'static str,
    firmware_release_date: &'static str,
    manufacturer: &'static str,
    product: &'static str,
    version: &'static str,
    family: &'static str,
    ram_base: u64,
    max_cpus: u64,
}

const VIRT_PLATFORM: SmbiosPlatform = SmbiosPlatform {
    firmware_vendor: "EDK II",
    firmware_version: "edk2-stable202408-prebuilt.qemu.org",
    firmware_release_date: "08/13/2024",
    manufacturer: "BridgeVM",
    product: "BridgeVM Virtual Machine",
    version: "virt",
    family: "Virtual Machine",
    ram_base: machine::RAM_BASE,
    max_cpus: machine::MAX_CPUS,
};

const BRIDGEVM_PC_PLATFORM: SmbiosPlatform = SmbiosPlatform {
    firmware_vendor: "Ketchio",
    firmware_version: "BridgeVM Firmware v1",
    firmware_release_date: "08/30/2026",
    manufacturer: bridgevm_pc::SMBIOS_MANUFACTURER,
    product: bridgevm_pc::SMBIOS_PRODUCT,
    version: "1",
    family: "BridgeVM Virtual ARM PC",
    ram_base: bridgevm_pc::RAM_BASE,
    max_cpus: bridgevm_pc::MAX_CPUS,
};

/// Build BridgeVM's SMBIOS surface for a `cpu_count`-CPU guest.
pub fn build_smbios(cpu_count: u64, ram_size: u64) -> SmbiosBlobs {
    build_smbios_for_platform(cpu_count, ram_size, VIRT_PLATFORM)
}

/// Build the independent BridgeVM Virtual ARM PC v1 SMBIOS surface.
pub fn build_bridgevm_pc_smbios(cpu_count: u64, ram_size: u64) -> SmbiosBlobs {
    assert!(bridgevm_pc::ram_region(ram_size).is_some());
    build_smbios_for_platform(cpu_count, ram_size, BRIDGEVM_PC_PLATFORM)
}
fn build_smbios_for_platform(
    cpu_count: u64,
    ram_size: u64,
    platform: SmbiosPlatform,
) -> SmbiosBlobs {
    assert!(cpu_count >= 1, "SMBIOS requires at least one CPU");
    assert!(
        cpu_count <= platform.max_cpus,
        "cpu_count {cpu_count} exceeds GICv3 redistributor window",
    );
    assert!(ram_size > 0, "SMBIOS requires non-zero RAM");
    platform
        .ram_base
        .checked_add(ram_size - 1)
        .expect("SMBIOS RAM range overflows guest physical address space");

    let mut tables = Vec::new();
    append_type0(&mut tables, platform);
    append_type1(&mut tables, platform);
    append_type3(&mut tables, platform);
    append_type4(&mut tables, cpu_count, platform);
    append_type16(&mut tables, ram_size);
    append_type17(&mut tables, ram_size, platform);
    append_type19(&mut tables, ram_size, platform.ram_base);
    append_type32(&mut tables);
    append_type127(&mut tables);

    let anchor = build_smbios30_anchor(tables.len());
    SmbiosBlobs { anchor, tables }
}

fn append_record(tables: &mut Vec<u8>, typ: u8, handle: u16, formatted: &[u8], strings: &[&str]) {
    let len = 4 + formatted.len();
    assert!(len <= u8::MAX as usize, "SMBIOS record too long");
    tables.push(typ);
    tables.push(len as u8);
    tables.extend_from_slice(&handle.to_le_bytes());
    tables.extend_from_slice(formatted);
    for s in strings {
        assert!(
            !s.as_bytes().contains(&0),
            "SMBIOS strings are NUL-terminated"
        );
        tables.extend_from_slice(s.as_bytes());
        tables.push(0);
    }
    tables.push(0);
    if strings.is_empty() {
        tables.push(0);
    }
}

fn append_type0(tables: &mut Vec<u8>, platform: SmbiosPlatform) {
    let mut f = Vec::new();
    f.push(1); // Vendor
    f.push(2); // BIOS Version
    f.extend_from_slice(&0xE800u16.to_le_bytes());
    f.push(3); // BIOS Release Date
    f.push(0); // BIOS ROM Size
    f.extend_from_slice(&0x08u64.to_le_bytes()); // BIOS characteristics: not supported
    f.push(0);
    f.push(0x1C); // TCD/SVVP + UEFI + virtual machine
    f.push(0);
    f.push(0);
    f.push(0xFF);
    f.push(0xFF);
    append_record(
        tables,
        0,
        HANDLE_TYPE0,
        &f,
        &[
            platform.firmware_vendor,
            platform.firmware_version,
            platform.firmware_release_date,
        ],
    );
}

fn append_type1(tables: &mut Vec<u8>, platform: SmbiosPlatform) {
    let mut f = vec![
        1, // Manufacturer
        2, // Product Name
        3, // Version
        4, // Serial Number
    ];
    f.extend_from_slice(&[0; 16]); // UUID unknown
    f.push(0x06); // Wake-up type: power switch
    f.push(0); // SKU
    f.push(5); // Family
    append_record(
        tables,
        1,
        HANDLE_TYPE1,
        &f,
        &[
            platform.manufacturer,
            platform.product,
            platform.version,
            "0",
            platform.family,
        ],
    );
}

fn append_type3(tables: &mut Vec<u8>, platform: SmbiosPlatform) {
    let mut f = vec![
        1,    // Manufacturer
        0x01, // Type: Other
        2,    // Version
        3,    // Serial Number
        0,    // Asset Tag
        0x03, // Boot-up state: safe
        0x03, // Power supply state: safe
        0x03, // Thermal state: safe
        0x02, // Security status: unknown
    ];
    f.extend_from_slice(&0u32.to_le_bytes());
    f.push(0); // Height
    f.push(0); // Number of power cords
    f.push(0); // Contained element count
    f.push(0); // Contained element record length
    f.push(0); // SKU
    append_record(
        tables,
        3,
        HANDLE_TYPE3,
        &f,
        &[platform.manufacturer, platform.version, "0"],
    );
}

fn append_type4(tables: &mut Vec<u8>, cpu_count: u64, platform: SmbiosPlatform) {
    let visible = cpu_count.min(u64::from(u16::MAX)) as u16;
    let visible_u8 = cpu_count.min(u64::from(u8::MAX)) as u8;

    let mut f = vec![
        1,    // Socket designation
        0x03, // Processor type: CPU
        0x01, // Processor family: Other
        2,    // Processor manufacturer
    ];
    f.extend_from_slice(&0u32.to_le_bytes()); // Processor ID
    f.extend_from_slice(&0u32.to_le_bytes());
    f.push(3); // Processor version
    f.push(0); // Voltage unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // External clock unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // Max speed unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // Current speed unknown
    f.push(0x41); // Socket populated, CPU enabled
    f.push(0x01); // Processor upgrade: Other
    f.extend_from_slice(&0xFFFFu16.to_le_bytes()); // L1 cache handle N/A
    f.extend_from_slice(&0xFFFFu16.to_le_bytes()); // L2 cache handle N/A
    f.extend_from_slice(&0xFFFFu16.to_le_bytes()); // L3 cache handle N/A
    f.push(0); // Serial
    f.push(0); // Asset
    f.push(0); // Part
    f.push(visible_u8); // Core count
    f.push(visible_u8); // Core enabled
    f.push(visible_u8); // Thread count
    f.extend_from_slice(&0x02u16.to_le_bytes()); // Processor characteristics: unknown
    f.extend_from_slice(&0x01u16.to_le_bytes()); // Processor family 2: Other
    f.extend_from_slice(&visible.to_le_bytes());
    f.extend_from_slice(&visible.to_le_bytes());
    f.extend_from_slice(&visible.to_le_bytes());
    append_record(
        tables,
        4,
        HANDLE_TYPE4,
        &f,
        &["CPU 0", platform.manufacturer, "Virtual CPU"],
    );
}

fn append_type16(tables: &mut Vec<u8>, ram_size: u64) {
    let size_kb = ram_size.div_ceil(KB);
    let mut f = Vec::new();
    f.push(0x01); // Location: Other
    f.push(0x03); // Use: system memory
    f.push(0x06); // Error correction: multi-bit ECC.
    if size_kb < MAX_TYPE16_STD_KB {
        f.extend_from_slice(&(size_kb as u32).to_le_bytes());
        f.extend_from_slice(&0xFFFEu16.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
    } else {
        f.extend_from_slice(&(MAX_TYPE16_STD_KB as u32).to_le_bytes());
        f.extend_from_slice(&0xFFFEu16.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&ram_size.to_le_bytes());
    }
    append_record(tables, 16, HANDLE_TYPE16, &f, &[]);
}

fn append_type17(tables: &mut Vec<u8>, ram_size: u64, platform: SmbiosPlatform) {
    let size_mb = ram_size.div_ceil(MB);
    let mut f = Vec::new();
    f.extend_from_slice(&HANDLE_TYPE16.to_le_bytes());
    f.extend_from_slice(&0xFFFEu16.to_le_bytes());
    f.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Total width unknown
    f.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Data width unknown
    if size_mb < MAX_TYPE17_STD_MB {
        f.extend_from_slice(&(size_mb as u16).to_le_bytes());
    } else {
        f.extend_from_slice(&(MAX_TYPE17_STD_MB as u16).to_le_bytes());
    }
    f.push(0x09); // Form factor: DIMM
    f.push(0); // Device set
    f.push(1); // Device locator
    f.push(0); // Bank locator
    f.push(0x07); // Memory type: RAM
    f.extend_from_slice(&0x02u16.to_le_bytes()); // Type detail: Other
    f.extend_from_slice(&0u16.to_le_bytes()); // Speed unknown
    f.push(2); // Manufacturer
    f.push(0); // Serial
    f.push(0); // Asset
    f.push(0); // Part
    f.push(0); // Attributes unknown
    let extended_mb = if size_mb < MAX_TYPE17_STD_MB {
        0
    } else {
        u32::try_from(size_mb).expect("SMBIOS memory device size exceeds 2 PiB")
    };
    f.extend_from_slice(&extended_mb.to_le_bytes());
    f.extend_from_slice(&0u16.to_le_bytes()); // Configured clock speed unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // Minimum voltage unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // Maximum voltage unknown
    f.extend_from_slice(&0u16.to_le_bytes()); // Configured voltage unknown
    append_record(
        tables,
        17,
        HANDLE_TYPE17,
        &f,
        &["DIMM 0", platform.manufacturer],
    );
}

fn append_type19(tables: &mut Vec<u8>, ram_size: u64, ram_base: u64) {
    let end = ram_base + ram_size - 1;
    let start_kb = ram_base / KB;
    let end_kb = end / KB;

    let mut f = Vec::new();
    if start_kb < u64::from(u32::MAX) && end_kb < u64::from(u32::MAX) {
        f.extend_from_slice(&(start_kb as u32).to_le_bytes());
        f.extend_from_slice(&(end_kb as u32).to_le_bytes());
        f.extend_from_slice(&HANDLE_TYPE16.to_le_bytes());
        f.push(1);
        f.extend_from_slice(&0u64.to_le_bytes());
        f.extend_from_slice(&0u64.to_le_bytes());
    } else {
        f.extend_from_slice(&u32::MAX.to_le_bytes());
        f.extend_from_slice(&u32::MAX.to_le_bytes());
        f.extend_from_slice(&HANDLE_TYPE16.to_le_bytes());
        f.push(1);
        f.extend_from_slice(&ram_base.to_le_bytes());
        f.extend_from_slice(&end.to_le_bytes());
    }
    append_record(tables, 19, HANDLE_TYPE19, &f, &[]);
}

fn append_type32(tables: &mut Vec<u8>) {
    append_record(tables, 32, HANDLE_TYPE32, &[0; 7], &[]);
}

fn append_type127(tables: &mut Vec<u8>) {
    append_record(tables, 127, HANDLE_TYPE127, &[], &[]);
}

fn build_smbios30_anchor(tables_len: usize) -> Vec<u8> {
    let mut a = Vec::with_capacity(24);
    a.extend_from_slice(b"_SM3_");
    a.push(0); // checksum: firmware recalculates after placing the table
    a.push(24); // entry point length
    a.push(3); // major
    a.push(0); // minor
    a.push(0); // doc revision
    a.push(1); // entry point revision
    a.push(0); // reserved
    a.extend_from_slice(
        &u32::try_from(tables_len)
            .expect("SMBIOS table blob exceeds 4 GiB")
            .to_le_bytes(),
    );
    a.extend_from_slice(&0u64.to_le_bytes()); // structure table address
    assert_eq!(a.len(), 24);
    a
}

#[cfg(test)]
#[path = "smbios_tests.rs"]
mod tests;
