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
fn anchor_has_smbios30_entry_point_shape() {
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

#[test]
fn bridgevm_pc_identity_and_ram_map_are_independent() {
    let blobs = build_bridgevm_pc_smbios(4, 512 * MB);
    let text = String::from_utf8_lossy(&blobs.tables);
    assert!(text.contains(bridgevm_pc::SMBIOS_MANUFACTURER));
    assert!(text.contains(bridgevm_pc::SMBIOS_PRODUCT));
    assert!(!text.contains("QEMU"));
    assert!(!text.contains("qemu"));
    assert!(!text.contains("EDK II"));
    assert!(!text.split('\0').any(|value| value == "virt"));

    let records = split_records(&blobs.tables);
    let type19 = records.iter().find(|record| record[0] == 19).unwrap();
    assert_eq!(le32(type19, 4), (bridgevm_pc::RAM_BASE / KB) as u32);
    assert_eq!(
        le32(type19, 8),
        ((bridgevm_pc::RAM_BASE + 512 * MB - 1) / KB) as u32
    );
    assert_eq!(le64(type19, 15), 0);
    assert_eq!(le64(type19, 23), 0);
}
