//! Logical request framing and completion lengths using a tiny synthetic medium.

use super::super::*;
#[path = "request_layout_test_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn split_header_uses_only_offered_bytes() {
    let (mut dev, mut mem) = setup();
    mem.write(HEADER + 8, &u64::MAX.to_le_bytes()); // Not part of the first descriptor.
    let used = run(
        &mut dev,
        &mut mem,
        &[
            (HEADER, 8, false),
            (SECTOR, 8, false),
            (DATA, 512, true),
            (STATUS, 1, true),
        ],
    );
    assert_eq!(mem.read(DATA, 512), [0x72; 512]);
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(used, 513);
}

#[test]
fn data_and_status_can_share_one_descriptor() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (DATA, 513, true)],
    );
    assert_eq!(mem.read(DATA, 512), [0x72; 512]);
    assert_eq!(mem.read(DATA + 512, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(used, 513);
}

#[test]
fn readable_after_writable_is_rejected_before_copying() {
    let (mut dev, mut mem) = setup();
    mem.write(HEADER + 8, &0u64.to_le_bytes());
    let used = run(
        &mut dev,
        &mut mem,
        &[
            (HEADER, 16, false),
            (DATA, 512, true),
            (SECTOR, 512, false),
            (STATUS, 1, true),
        ],
    );
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(dev.stats.bytes_read, 0);
    assert_eq!(used, 0);
}

#[test]
fn short_header_is_not_read_from_adjacent_padding() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 8, false), (DATA, 512, true), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_IOERR]);
    assert_eq!(used, 0);
}

#[test]
fn unaligned_data_length_is_rejected_before_copying() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (DATA, 511, true), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_IOERR]);
    assert_eq!(used, 0);
}

#[test]
fn failed_status_write_is_not_counted_as_written() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (DATA, 512, true), (0x7000, 1, true)],
    );
    assert_eq!(mem.read(DATA, 512), [0x72; 512]);
    assert_eq!(used, 512);
}

#[test]
fn scattered_data_and_empty_trailer_preserve_last_status_byte() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[
            (HEADER, 16, false),
            (DATA, 100, true),
            (DATA + 200, 413, true),
            (u64::MAX, 0, true),
        ],
    );
    assert_eq!(mem.read(DATA, 100), [0x72; 100]);
    assert_eq!(mem.read(DATA + 100, 100), [0xa5; 100]);
    assert_eq!(mem.read(DATA + 200, 412), [0x72; 412]);
    assert_eq!(mem.read(DATA + 612, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(used, 513);
}

#[test]
fn overflowing_late_span_is_rejected_before_any_copy() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[
            (HEADER, 16, false),
            (DATA, 256, true),
            (u64::MAX - 127, 256, true),
            (STATUS, 1, true),
        ],
    );
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(mem.read(STATUS, 1), [0xa5]);
    assert_eq!(used, 0);
}

#[test]
fn partial_copy_reports_only_initialized_prefix() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[
            (HEADER, 16, false),
            (DATA, 256, true),
            (0x7000, 256, true),
            (STATUS, 1, true),
        ],
    );
    assert_eq!(mem.read(DATA, 256), [0x72; 256]);
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_IOERR]);
    assert_eq!(dev.stats.bytes_read, 256);
    assert_eq!(used, 256);
}

#[test]
fn mapped_zero_address_can_hold_status() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (DATA, 512, true), (0, 1, true)],
    );
    assert_eq!(mem.read(0, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(used, 513);
}

#[test]
fn zero_length_read_writes_only_status() {
    let (mut dev, mut mem) = setup();
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(used, 1);
}

#[test]
fn unterminated_chain_has_no_completion_write() {
    let (mut dev, mut mem) = setup();
    dev.queue_num = 1;
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(STATUS, 1), [0xa5]);
    assert_eq!(used, 0);
}

#[test]
fn read_only_write_request_returns_ioerr() {
    let (mut dev, mut mem) = setup();
    mem.write(HEADER, &1u32.to_le_bytes());
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (DATA, 512, false), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_IOERR]);
    assert_eq!(mem.read(DATA, 512), [0xa5; 512]);
    assert_eq!(dev.stats.bytes_read, 0);
    assert_eq!(dev.recent_request_trace().last().unwrap().data_len, 512);
    assert_eq!(used, 1);
}

#[test]
fn unknown_request_remains_unsupported() {
    let (mut dev, mut mem) = setup();
    mem.write(HEADER, &u32::MAX.to_le_bytes());
    let used = run(
        &mut dev,
        &mut mem,
        &[(HEADER, 16, false), (STATUS, 1, true)],
    );
    assert_eq!(mem.read(STATUS, 1), [VIRTIO_BLK_S_UNSUPP]);
    assert_eq!(used, 1);
}
