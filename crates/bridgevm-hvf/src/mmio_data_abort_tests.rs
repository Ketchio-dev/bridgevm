use super::*;

fn read(size_log2: u64, signed: bool, register_64: bool) -> DataAbort {
    DataAbort::decode(
        (0x24 << 26)
            | (1 << 24)
            | (size_log2 << 22)
            | (u64::from(signed) << 21)
            | (3 << 16)
            | (u64::from(register_64) << 15),
    )
    .unwrap()
}

#[test]
fn rejects_invalid_instruction_syndrome_before_mmio() {
    for fields in [0, (3 << 22) | (9 << 16) | (1 << 6)] {
        assert!(DataAbort::decode((0x24 << 26) | fields).is_err());
    }
}

#[test]
fn signed_loads_extend_to_x_register_width() {
    for (size, input, expected) in [
        (0, 0x80, 0xffff_ffff_ffff_ff80),
        (1, 0x8000, 0xffff_ffff_ffff_8000),
        (2, 0x8000_0000, 0xffff_ffff_8000_0000),
    ] {
        assert_eq!(read(size, true, true).read_result(input), Some(expected));
    }
}

#[test]
fn signed_loads_to_w_register_clear_the_upper_half() {
    for (size, input, expected) in [(0, 0x80, 0xffff_ff80), (1, 0x8000, 0xffff_8000)] {
        assert_eq!(read(size, true, false).read_result(input), Some(expected));
    }
}

#[test]
fn unsigned_loads_mask_device_value_to_access_width() {
    for (size, expected) in [(0, 0xff), (1, 0xffff), (2, 0xffff_ffff), (3, u64::MAX)] {
        assert_eq!(
            read(size, false, true).read_result(u64::MAX),
            Some(expected)
        );
    }
}

#[test]
fn positive_signed_loads_do_not_extend() {
    assert_eq!(read(0, true, true).read_result(0x7f), Some(0x7f));
    assert_eq!(read(1, true, false).read_result(0x7fff), Some(0x7fff));
}

#[test]
fn destination_width_also_applies_to_unsigned_and_positive_reads() {
    assert_eq!(
        read(3, false, false).read_result(u64::MAX),
        Some(0xffff_ffff)
    );
    assert_eq!(read(3, true, false).read_result(0x1_0000_0000), Some(0));
    assert_eq!(
        read(2, false, false).read_result(u64::MAX),
        Some(0xffff_ffff)
    );
}

#[test]
fn zero_register_and_stores_have_no_read_writeback() {
    let zero = DataAbort::decode((0x24 << 26) | (1 << 24) | (31 << 16)).unwrap();
    let store = DataAbort::decode((0x24 << 26) | (1 << 24) | (2 << 16) | (1 << 6)).unwrap();
    assert_eq!(zero.register, 31);
    assert_eq!(zero.read_result(u64::MAX), None);
    assert!(store.write);
    assert_eq!(store.read_result(u64::MAX), None);
}

#[test]
fn rejects_other_exception_classes() {
    assert!(DataAbort::decode((0x16 << 26) | (1 << 24)).is_err());
}
