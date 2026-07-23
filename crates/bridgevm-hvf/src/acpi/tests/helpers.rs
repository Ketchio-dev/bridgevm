//! Split test module.

use super::super::*;

/// Sum of every byte must be zero (mod 256) for any valid ACPI structure.
pub(super) fn sums_to_zero(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b)) == 0
}

/// Read a little-endian u16 at `off`.
pub(super) fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

/// Read a little-endian u32 at `off`.
pub(super) fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Read a little-endian u64 at `off`.
pub(super) fn le64(b: &[u8], off: usize) -> u64 {
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

pub(super) fn le_name(b: &[u8], off: usize) -> String {
    let name = &b[off..off + LOADER_FILE_NAME_LEN];
    let len = name
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(LOADER_FILE_NAME_LEN);
    std::str::from_utf8(&name[..len]).unwrap().to_string()
}

pub(super) fn read_le_sized(b: &[u8], off: usize, size: u8) -> u64 {
    let mut raw = [0u8; 8];
    raw[..size as usize].copy_from_slice(&b[off..off + size as usize]);
    u64::from_le_bytes(raw)
}

pub(super) fn write_le_sized(b: &mut [u8], off: usize, size: u8, value: u64) {
    b[off..off + size as usize].copy_from_slice(&value.to_le_bytes()[..size as usize]);
}

pub(super) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

pub(super) const TEST_TPM_LOG_BASE: u64 = 0x4900_0000;

pub(super) fn replay_loader(blobs: &AcpiBlobs) -> (Vec<u8>, Vec<u8>) {
    const TABLES_BASE: u64 = 0x4800_0000;
    const RSDP_BASE: u64 = 0x000F_0000;

    let mut tables = Vec::new();
    let mut rsdp = Vec::new();

    for entry in blobs.loader.chunks_exact(LOADER_ENTRY_LEN) {
        let cmd = le32(entry, 0);
        match cmd {
            LOADER_CMD_ALLOCATE => {
                let file = le_name(entry, 4);
                let align = le32(entry, 4 + LOADER_FILE_NAME_LEN);
                let zone = entry[4 + LOADER_FILE_NAME_LEN + 4];
                match file.as_str() {
                    ACPI_RSDP_FILE => {
                        assert_eq!(align, RSDP_ALLOC_ALIGN);
                        assert_eq!(zone, LOADER_ZONE_FSEG);
                        rsdp = blobs.rsdp.clone();
                    }
                    ACPI_TABLE_FILE => {
                        assert_eq!(align, TABLE_ALLOC_ALIGN);
                        assert_eq!(zone, LOADER_ZONE_HIGH);
                        tables = blobs.tables.clone();
                    }
                    ACPI_TPM_LOG_FILE => {
                        assert_eq!(align, 1);
                        assert_eq!(zone, LOADER_ZONE_HIGH);
                        assert_eq!(
                            blobs.tpm_log.as_ref().map(Vec::len),
                            Some(TPM_LOG_AREA_MINIMUM_SIZE)
                        );
                    }
                    other => panic!("unexpected ACPI allocation {other}"),
                }
            }
            LOADER_CMD_ADD_POINTER => {
                let pointer_file = le_name(entry, 4);
                let pointee_file = le_name(entry, 4 + LOADER_FILE_NAME_LEN);
                let off = 4 + LOADER_FILE_NAME_LEN * 2;
                let pointer_offset = le32(entry, off) as usize;
                let pointer_size = entry[off + 4];
                let (pointee_base, pointee_size) = match pointee_file.as_str() {
                    ACPI_TABLE_FILE => (TABLES_BASE, tables.len()),
                    ACPI_RSDP_FILE => (RSDP_BASE, rsdp.len()),
                    ACPI_TPM_LOG_FILE => (
                        TEST_TPM_LOG_BASE,
                        blobs.tpm_log.as_ref().expect("TPM log allocation").len(),
                    ),
                    other => panic!("unexpected pointer source {other}"),
                };
                let target = match pointer_file.as_str() {
                    ACPI_TABLE_FILE => &mut tables,
                    ACPI_RSDP_FILE => &mut rsdp,
                    other => panic!("unexpected pointer destination {other}"),
                };
                let value = read_le_sized(target, pointer_offset, pointer_size);
                assert!(
                    value < pointee_size as u64,
                    "pointer offset {value:#x} must point inside {pointee_file}",
                );
                write_le_sized(target, pointer_offset, pointer_size, value + pointee_base);
            }
            LOADER_CMD_ADD_CHECKSUM => {
                let file = le_name(entry, 4);
                let off = 4 + LOADER_FILE_NAME_LEN;
                let result = le32(entry, off) as usize;
                let start = le32(entry, off + 4) as usize;
                let len = le32(entry, off + 8) as usize;
                let target = match file.as_str() {
                    ACPI_TABLE_FILE => &mut tables,
                    ACPI_RSDP_FILE => &mut rsdp,
                    other => panic!("unexpected checksum file {other}"),
                };
                target[result] = checksum(&target[start..start + len]);
            }
            other => panic!("unexpected ACPI loader command {other}"),
        }
    }

    assert!(!tables.is_empty(), "table allocation command missing");
    assert!(!rsdp.is_empty(), "RSDP allocation command missing");
    (rsdp, tables)
}

/// Split the `etc/acpi/tables` blob back into (signature, slice) tables by
/// walking each header's length field.
pub(super) fn split_tables(tables: &[u8]) -> Vec<(String, &[u8])> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + ACPI_HEADER_LEN <= tables.len() {
        let sig = String::from_utf8_lossy(&tables[off..off + 4]).to_string();
        let len = le32(tables, off + 4) as usize;
        assert!(
            len >= ACPI_HEADER_LEN,
            "table {sig} length too small: {len}"
        );
        assert!(off + len <= tables.len(), "table {sig} overruns blob");
        out.push((sig, &tables[off..off + len]));
        off += len;
    }
    assert_eq!(off, tables.len(), "tables blob has trailing bytes");
    out
}

pub(super) fn find<'a>(tables: &'a [(String, &'a [u8])], sig: &str) -> &'a [u8] {
    tables
        .iter()
        .find(|(s, _)| s == sig)
        .unwrap_or_else(|| panic!("missing table {sig}"))
        .1
}

#[test]
fn optional_tpm_emits_tpm2_table_and_relocated_event_log() {
    let blobs = build_acpi_with_devices(
        2,
        AcpiDeviceConfig {
            tpm_tis_present: true,
        },
    );
    let log = blobs.tpm_log.as_ref().expect("TPM log must be present");
    assert_eq!(log.len(), TPM_LOG_AREA_MINIMUM_SIZE);
    assert!(log.iter().all(|byte| *byte == 0));

    let (_, relocated_tables) = replay_loader(&blobs);
    let tables = split_tables(&relocated_tables);
    let tpm2 = find(&tables, "TPM2");
    assert_eq!(tpm2.len(), 76);
    assert_eq!(tpm2[8], 4, "TPM2 table revision");
    assert_eq!(le16(tpm2, 36), 0, "client platform class");
    assert_eq!(le64(tpm2, 40), 0, "FIFO has no control area");
    assert_eq!(le32(tpm2, 48), 6, "MMIO start method");
    assert_eq!(le32(tpm2, 64) as usize, TPM_LOG_AREA_MINIMUM_SIZE, "LAML");
    assert_eq!(le64(tpm2, 68), TEST_TPM_LOG_BASE, "relocated LASA");
    assert!(sums_to_zero(tpm2));

    let xsdt = find(&tables, "XSDT");
    assert_eq!((xsdt.len() - ACPI_HEADER_LEN) / 8, 8);
    assert!(blobs.loader.chunks_exact(LOADER_ENTRY_LEN).any(|entry| {
        le32(entry, 0) == LOADER_CMD_ALLOCATE && le_name(entry, 4) == ACPI_TPM_LOG_FILE
    }));
}
