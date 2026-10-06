//! Public admin SQ/CQ fixtures for the existing log and discovery payloads.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

pub(super) const SENTINEL: u8 = 0xa5;
pub(super) const DATA_LEN: usize = 6 * PAGE_SIZE;
pub(super) const SECOND_PAGE: u64 = DATA_BASE + 4 * PAGE_SIZE_U64;
pub(super) const UNBACKED: u64 = MEM_BASE + 0x10000;

#[derive(Clone, Copy, Debug)]
pub(super) struct Case {
    pub(super) opcode: u8,
    pub(super) selector: u32,
    pub(super) len: usize,
}

pub(super) const CASES: [Case; 4] = [
    Case {
        opcode: ADMIN_OP_GET_LOG_PAGE,
        selector: LOG_PAGE_SMART_HEALTH as u32,
        len: 512,
    },
    Case {
        opcode: ADMIN_OP_GET_LOG_PAGE,
        selector: LOG_PAGE_FIRMWARE_SLOT_INFO as u32,
        len: 512,
    },
    Case {
        opcode: ADMIN_OP_GET_LOG_PAGE,
        selector: LOG_PAGE_COMMAND_EFFECTS as u32,
        len: PAGE_SIZE,
    },
    Case {
        opcode: ADMIN_OP_SECURITY_RECV,
        selector: 0,
        len: 10,
    },
];

pub(super) fn fixture() -> (NvmeController, FakeMem) {
    let (ctrl, mut mem) = enabled_controller_with_mem_len(0x10000);
    assert!(mem.write_bytes(DATA_BASE, &[SENTINEL; DATA_LEN]));
    (ctrl, mem)
}

pub(super) fn submit(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    case: Case,
    prps: (u64, u64),
) -> u16 {
    let (nsid, words) = if case.opcode == ADMIN_OP_GET_LOG_PAGE {
        (
            u32::MAX,
            [case.selector | (((case.len / 4 - 1) as u32) << 16), 0, 0],
        )
    } else {
        (0, [case.selector, case.len as u32, 0])
    };
    let command = encode_sqe_with_prps(case.opcode, 0x81, nsid, prps.0, prps.1, words);
    submit_admin(ctrl, mem, 0, &command);
    let completion = read_completion(mem, ACQ_BASE, 0);
    assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), 0x81);
    assert_eq!(u16::from_le_bytes([completion[8], completion[9]]), 1);
    assert_eq!(u16::from_le_bytes([completion[10], completion[11]]), 0);
    assert_eq!(completion[14] & 1, 1);
    completion_status(&completion)
}

pub(super) fn reference(case: Case) -> Vec<u8> {
    let (mut ctrl, mut mem) = fixture();
    assert_eq!(
        submit(&mut ctrl, &mut mem, case, (DATA_BASE, 0)),
        SC_SUCCESS
    );
    let data = mem.read_bytes(DATA_BASE, case.len).unwrap();
    match (case.opcode, case.selector) {
        (ADMIN_OP_GET_LOG_PAGE, 2) => {
            assert_eq!(&data[..5], &[0, 44, 1, 100, 10]);
        }
        (ADMIN_OP_GET_LOG_PAGE, 3) => {
            assert_eq!(data[0], 1);
            assert!(data[8..72].starts_with(b"BridgeVM NVMe firmware slot 1"));
        }
        (ADMIN_OP_GET_LOG_PAGE, 5) => {
            assert_eq!(&data[24..28], &1u32.to_le_bytes());
            assert_eq!(&data[1028..1032], &3u32.to_le_bytes());
        }
        (ADMIN_OP_SECURITY_RECV, 0) => {
            assert_eq!(data, [0, 0, 0, 0, 0, 0, 0, 2, 0, 0]);
        }
        _ => unreachable!(),
    }
    data
}

pub(super) fn assert_data_region(mem: &FakeMem, expected: &[u8]) {
    let actual = mem.read_bytes(DATA_BASE, DATA_LEN).unwrap();
    assert_eq!(expected.len(), DATA_LEN);
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "data-region byte={index}");
    }
}

pub(super) fn scattered(case: Case) {
    let data = reference(case);
    let offsets: &[usize] = match case.len {
        PAGE_SIZE => &[4, 512, PAGE_SIZE - 4],
        512 => &[PAGE_SIZE - 512 + 4, PAGE_SIZE - 256, PAGE_SIZE - 4],
        10 => &[PAGE_SIZE - 8, PAGE_SIZE - 4],
        _ => unreachable!(),
    };
    for &offset in offsets {
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                case,
                (DATA_BASE + offset as u64, SECOND_PAGE)
            ),
            SC_SUCCESS
        );
        let first_len = PAGE_SIZE - offset;
        let mut expected = vec![SENTINEL; DATA_LEN];
        expected[offset..PAGE_SIZE].copy_from_slice(&data[..first_len]);
        expected[4 * PAGE_SIZE..4 * PAGE_SIZE + case.len - first_len]
            .copy_from_slice(&data[first_len..]);
        assert_data_region(&mem, &expected);
    }
}

pub(super) fn failed_second(cases: &[Case], prp2: u64, structurally_invalid: bool) {
    for &case in cases {
        let (mut ctrl, mut mem) = fixture();
        assert_eq!(
            submit(
                &mut ctrl,
                &mut mem,
                case,
                (DATA_BASE + PAGE_SIZE_U64 - 4, prp2)
            ),
            SC_INVALID_FIELD
        );
        if structurally_invalid {
            assert_data_region(&mem, &[SENTINEL; DATA_LEN]);
        } else {
            // A failed later DMA may leave the first intended span written.
            assert_eq!(
                mem.read_bytes(DATA_BASE + PAGE_SIZE_U64, DATA_LEN - PAGE_SIZE)
                    .unwrap(),
                vec![SENTINEL; DATA_LEN - PAGE_SIZE]
            );
        }
    }
}
