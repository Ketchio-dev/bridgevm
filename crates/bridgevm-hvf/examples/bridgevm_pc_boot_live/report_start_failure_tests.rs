use super::*;

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn accepts_a_bounded_utf16_exit_record() {
    let mut ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    let record = &mut ram[OFFSET..];
    put_u64(record, 0, MAGIC);
    put_u32(record, 8, VERSION);
    put_u32(record, 12, 2);
    put_u64(record, 16, 0x8000_0000_0000_0002);
    put_u64(record, 24, 6);
    put_u64(record, 32, 0x1000_4000);
    record[HEADER..HEADER + 4].copy_from_slice(&[b'O', 0, b'K', 0]);
    let failure = decode(&ram).unwrap().unwrap();
    assert_eq!(failure.status, 0x8000_0000_0000_0002);
    assert_eq!(failure.exit_data_size, 6);
    assert_eq!(failure.exit_data_address, 0x1000_4000);
    assert_eq!(failure.units, vec![b'O' as u16, b'K' as u16]);
}

#[test]
fn rejects_an_oversized_unit_count() {
    let mut ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    let record = &mut ram[OFFSET..];
    put_u64(record, 0, MAGIC);
    put_u32(record, 8, VERSION);
    put_u32(record, 12, CAPACITY as u32 + 1);
    assert!(decode(&ram).unwrap_err().contains("units=97"));
}

#[test]
fn treats_an_unwritten_record_as_absent() {
    let ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    assert_eq!(decode(&ram).unwrap(), None);
}
