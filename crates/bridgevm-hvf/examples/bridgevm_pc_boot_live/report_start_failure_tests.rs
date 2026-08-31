use super::*;

fn put(bytes: &mut [u8], offset: usize, value: u64, len: usize) {
    bytes[offset..offset + len].copy_from_slice(&value.to_le_bytes()[..len]);
}

#[test]
fn accepts_a_bounded_utf16_exit_record() {
    let mut ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    let record = &mut ram[OFFSET..];
    put(record, 0, MAGIC, 8);
    put(record, 8, VERSION as u64, 4);
    put(record, 12, 2, 4);
    put(record, 16, 0x8000_0000_0000_0002, 8);
    put(record, 24, 6, 8);
    put(record, 32, 0x1000_4000, 8);
    put(record, 40, 0x8000_0000_0000_0003, 8);
    record[HEADER..HEADER + 4].copy_from_slice(&[b'O', 0, b'K', 0]);
    let failure = decode(&ram).unwrap().unwrap();
    assert_eq!(failure.status, 0x8000_0000_0000_0002);
    assert_eq!(failure.exit_data_size, 6);
    assert_eq!(failure.exit_data_address, 0x1000_4000);
    assert_eq!(failure.loaded_image_probe_status, 0x8000_0000_0000_0003);
    assert_eq!(failure.units, vec![b'O' as u16, b'K' as u16]);
}

#[test]
fn rejects_an_oversized_unit_count() {
    let mut ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    put(&mut ram[OFFSET..], 0, MAGIC, 8);
    put(&mut ram[OFFSET..], 8, VERSION as u64, 4);
    put(&mut ram[OFFSET..], 12, CAPACITY as u64 + 1, 4);
    assert!(decode(&ram).unwrap_err().contains("units=97"));
}

#[test]
fn treats_an_unwritten_record_as_absent() {
    let ram = vec![0; OFFSET + HEADER + CAPACITY * 2];
    assert_eq!(decode(&ram).unwrap(), None);
}
