//! Minimal SMBIOS blobs for the Path A `fw_cfg` handoff.
//!
//! ArmVirtQemu's SMBIOS platform driver reads QEMU-compatible records from
//! `etc/smbios/smbios-tables` and installs them through EFI's SMBIOS protocol.
//! QEMU also exposes an `etc/smbios/smbios-anchor` entry point blob; its address
//! and checksum fields are deliberately zero because firmware finalizes them
//! after placing the SMBIOS tables.

use crate::machine;

/// QEMU fw_cfg file carrying the SMBIOS 3.0 entry point.
pub const SMBIOS_ANCHOR_FILE: &str = "etc/smbios/smbios-anchor";
/// QEMU fw_cfg file carrying concatenated SMBIOS structures.
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

/// Build a small QEMU-style SMBIOS surface for a `cpu_count`-CPU guest.
pub fn build_smbios(cpu_count: u64, ram_size: u64) -> SmbiosBlobs {
    assert!(cpu_count >= 1, "SMBIOS requires at least one CPU");
    assert!(
        machine::redist_fits(cpu_count),
        "cpu_count {cpu_count} exceeds GICv3 redistributor window",
    );
    assert!(ram_size > 0, "SMBIOS requires non-zero RAM");

    let mut tables = Vec::new();
    append_type0(&mut tables);
    append_type1(&mut tables);
    append_type3(&mut tables);
    append_type4(&mut tables, cpu_count);
    append_type16(&mut tables, ram_size);
    append_type17(&mut tables, ram_size);
    append_type19(&mut tables, ram_size);
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

fn append_type0(tables: &mut Vec<u8>) {
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
            "EDK II",
            "edk2-stable202408-prebuilt.qemu.org",
            "08/13/2024",
        ],
    );
}

fn append_type1(tables: &mut Vec<u8>) {
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
            "BridgeVM",
            "BridgeVM Virtual Machine",
            "virt",
            "0",
            "Virtual Machine",
        ],
    );
}

fn append_type3(tables: &mut Vec<u8>) {
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
    append_record(tables, 3, HANDLE_TYPE3, &f, &["BridgeVM", "virt", "0"]);
}

fn append_type4(tables: &mut Vec<u8>, cpu_count: u64) {
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
        &["CPU 0", "BridgeVM", "Virtual CPU"],
    );
}

fn append_type16(tables: &mut Vec<u8>, ram_size: u64) {
    let size_kb = ram_size.div_ceil(KB);
    let mut f = Vec::new();
    f.push(0x01); // Location: Other
    f.push(0x03); // Use: system memory
    f.push(0x06); // Error correction: multi-bit ECC, matching QEMU/SeaBIOS
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

fn append_type17(tables: &mut Vec<u8>, ram_size: u64) {
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
    append_record(tables, 17, HANDLE_TYPE17, &f, &["DIMM 0", "BridgeVM"]);
}

fn append_type19(tables: &mut Vec<u8>, ram_size: u64) {
    let end = machine::RAM_BASE + ram_size - 1;
    let start_kb = machine::RAM_BASE / KB;
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
        f.extend_from_slice(&machine::RAM_BASE.to_le_bytes());
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
    debug_assert_eq!(a.len(), 24);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le16(b: &[u8], off: usize) -> u16 {
        u16::from_le_bytes([b[off], b[off + 1]])
    }
    fn le32(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }
    fn le64(b: &[u8], off: usize) -> u64 {
        u64::from_le_bytes([
            b[off],
            b[off + 1],
            b[off + 2],
            b[off + 3],
            b[off + 4],
            b[off + 5],
            b[off + 6],
            b[off + 7],
        ])
    }

    fn split_records(tables: &[u8]) -> Vec<&[u8]> {
        let mut out = Vec::new();
        let mut off = 0usize;
        while off < tables.len() {
            assert!(off + 4 <= tables.len(), "truncated SMBIOS record header");
            let len = tables[off + 1] as usize;
            assert!(len >= 4, "SMBIOS record length too small");
            let mut end = off + len;
            while end + 1 < tables.len() && (tables[end] != 0 || tables[end + 1] != 0) {
                end += 1;
            }
            assert!(end + 1 < tables.len(), "SMBIOS record missing double NUL");
            end += 2;
            out.push(&tables[off..end]);
            off = end;
        }
        out
    }

    #[test]
    fn anchor_has_qemu_smbios30_shape() {
        let blobs = build_smbios(1, 512 * MB);
        assert_eq!(&blobs.anchor[..5], b"_SM3_");
        assert_eq!(blobs.anchor[5], 0, "firmware owns final checksum");
        assert_eq!(blobs.anchor[6], 24);
        assert_eq!(blobs.anchor[7], 3);
        assert_eq!(blobs.anchor[8], 0);
        assert_eq!(blobs.anchor[10], 1);
        assert_eq!(le32(&blobs.anchor, 12), blobs.tables.len() as u32);
        assert_eq!(le64(&blobs.anchor, 16), 0, "firmware owns final address");
    }

    #[test]
    fn tables_have_expected_required_records() {
        let blobs = build_smbios(1, 512 * MB);
        let records = split_records(&blobs.tables);
        let types: Vec<u8> = records.iter().map(|record| record[0]).collect();
        assert_eq!(types, [0, 1, 3, 4, 16, 17, 19, 32, 127]);
        assert!(String::from_utf8_lossy(&blobs.tables).contains("BridgeVM Virtual Machine"));
        assert_eq!(records.last().unwrap()[0], 127);
    }

    #[test]
    fn memory_records_describe_guest_ram() {
        let blobs = build_smbios(1, 512 * MB);
        let records = split_records(&blobs.tables);
        let type16 = records.iter().find(|record| record[0] == 16).unwrap();
        let type17 = records.iter().find(|record| record[0] == 17).unwrap();
        let type19 = records.iter().find(|record| record[0] == 19).unwrap();

        assert_eq!(le32(type16, 7), 512 * 1024);
        assert_eq!(le16(type16, 13), 1);
        assert_eq!(le16(type17, 12), 512);
        assert_eq!(le32(type19, 4), (machine::RAM_BASE / KB) as u32);
        assert_eq!(
            le32(type19, 8),
            ((machine::RAM_BASE + 512 * MB - 1) / KB) as u32
        );
    }

    #[test]
    fn processor_record_scales_with_cpu_count() {
        let blobs = build_smbios(4, 512 * MB);
        let records = split_records(&blobs.tables);
        let type4 = records.iter().find(|record| record[0] == 4).unwrap();
        assert_eq!(type4[35], 4, "core count");
        assert_eq!(type4[36], 4, "core enabled");
        assert_eq!(type4[37], 4, "thread count");
        assert_eq!(le16(type4, 42), 4, "core count 2");
        assert_eq!(le16(type4, 44), 4, "core enabled 2");
        assert_eq!(le16(type4, 46), 4, "thread count 2");
    }
}
