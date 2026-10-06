//! Regressions for transfer-derived PRP list bounds.

use super::super::*;
use super::prp_limit_support::*;
use crate::fwcfg::GuestMemoryMut;

fn successful_transfer(
    bytes: usize,
    offsets: (usize, usize),
    opcode: u8,
    direct: bool,
    lists: usize,
) {
    let mut fixture = Fixture::new(
        Transfer {
            bytes,
            first_offset: offsets.0,
            list_offset: offsets.1,
        },
        opcode,
        direct,
    );
    assert_eq!(
        fixture.lists.len(),
        lists,
        "independent packed-list geometry"
    );
    fixture.submit(SC_SUCCESS);
    fixture.assert_payload_and_guards();
}

macro_rules! cases {
    ($read:ident, $write:ident, $read_direct:ident, $write_direct:ident, $bytes:expr, $offsets:expr, $lists:expr) => {
        #[test]
        fn $read() {
            successful_transfer($bytes, $offsets, NVM_OP_READ, false, $lists);
        }
        #[test]
        fn $write() {
            successful_transfer($bytes, $offsets, NVM_OP_WRITE, false, $lists);
        }
        #[test]
        fn $read_direct() {
            successful_transfer($bytes, $offsets, NVM_OP_READ, true, $lists);
        }
        #[test]
        fn $write_direct() {
            successful_transfer($bytes, $offsets, NVM_OP_WRITE, true, $lists);
        }
    };
}

cases!(
    sixteen_read,
    sixteen_write,
    sixteen_direct_read,
    sixteen_direct_write,
    SIXTEEN_LIST_PAGES,
    (0, 0),
    16
);
cases!(
    next_lba_read,
    next_lba_write,
    next_lba_direct_read,
    next_lba_direct_write,
    SIXTEEN_LIST_PAGES + LBA_SIZE,
    (0, 0),
    17
);
cases!(
    maximum_read,
    maximum_write,
    maximum_direct_read,
    maximum_direct_write,
    MAX_TRANSFER,
    (0, 0),
    17
);
cases!(
    offset_maximum_read,
    offset_maximum_write,
    offset_maximum_direct_read,
    offset_maximum_direct_write,
    MAX_TRANSFER,
    (PAGE_SIZE - 4, PAGE_SIZE - 8),
    18
);

fn rejected_late_chain(opcode: u8, direct: bool) {
    for bad in [0, 0x4000_8004, 0x7000_0000] {
        let mut fixture = Fixture::new(
            Transfer {
                bytes: MAX_TRANSFER,
                first_offset: PAGE_SIZE - 4,
                list_offset: PAGE_SIZE - 8,
            },
            opcode,
            direct,
        );
        // Corrupt page 17's link to page 18, after the old arbitrary bound.
        let chain = fixture.lists[16] + PAGE_SIZE_U64 - 8;
        assert!(fixture.mem.write_bytes(chain, &u64::to_le_bytes(bad)));
        let ram = fixture.mem.bytes.clone();
        let disk = fixture.ctrl.disk_image().to_vec();
        let command = SubmissionEntry::from_bytes(&fixture.command);
        let sentinel = (0x1234, 7);
        let mut spans = vec![sentinel];
        assert!(!prp_spans_into(
            &command,
            MAX_TRANSFER,
            &fixture.mem,
            &mut spans,
            &mut [0; PAGE_SIZE]
        ));
        assert_eq!(spans, [sentinel], "rejection preserves the caller's prefix");
        fixture.submit(SC_INVALID_FIELD);
        fixture.assert_unchanged(&ram, &disk);
    }
}

#[test]
fn late_chain_read_rejected() {
    rejected_late_chain(NVM_OP_READ, false);
}
#[test]
fn late_chain_write_rejected() {
    rejected_late_chain(NVM_OP_WRITE, false);
}
#[test]
fn late_chain_direct_read_rejected() {
    rejected_late_chain(NVM_OP_READ, true);
}
#[test]
fn late_chain_direct_write_rejected() {
    rejected_late_chain(NVM_OP_WRITE, true);
}
