//! Public admin-queue fixtures for Identify DMA placement.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

pub(super) const SENTINEL: u8 = 0xa5;
pub(super) const DATA_LEN: usize = 6 * PAGE_SIZE;
pub(super) const SECOND_PAGE: u64 = DATA_BASE + 4 * PAGE_SIZE_U64;
pub(super) const UNBACKED: u64 = MEM_BASE + 0x10000;
pub(super) const CASES: [(u32, u32); 9] = [
    (IDENTIFY_CNS_CONTROLLER, 0),
    (IDENTIFY_CNS_NAMESPACE, NSID),
    (IDENTIFY_CNS_NAMESPACE, NSID2),
    (IDENTIFY_CNS_NAMESPACE, 99),
    (IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST, 0),
    (IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST, NSID),
    (IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST, NSID2),
    (IDENTIFY_CNS_COMMAND_SET_CONTROLLER, 0),
    (IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST, NSID),
];

pub(super) fn fixture() -> (NvmeController, FakeMem) {
    let (mut ctrl, mem) = enabled_controller_with_mem_len(0x10000);
    ctrl.attach_second_namespace(3 * LBA_SIZE);
    (ctrl, mem)
}

pub(super) fn fill_data(mem: &mut FakeMem) {
    assert!(mem.write_bytes(DATA_BASE, &[SENTINEL; DATA_LEN]));
}

pub(super) fn submit(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    kind: (u32, u32),
    prps: (u64, u64),
) -> u16 {
    let cid = 0x50 + slot;
    let command = encode_sqe_with_prps(
        ADMIN_OP_IDENTIFY,
        cid,
        kind.1,
        prps.0,
        prps.1,
        [kind.0, 0, 0],
    );
    submit_admin(ctrl, mem, slot, &command);
    let completion = read_completion(mem, ACQ_BASE, slot);
    assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), cid);
    assert_eq!(u16::from_le_bytes([completion[8], completion[9]]), slot + 1);
    assert_eq!(u16::from_le_bytes([completion[10], completion[11]]), 0);
    assert_eq!(completion[14] & 1, 1);
    completion_status(&completion)
}

pub(super) fn reference(kind: (u32, u32)) -> Vec<u8> {
    let (mut ctrl, mut mem) = fixture();
    fill_data(&mut mem);
    assert_eq!(
        submit(&mut ctrl, &mut mem, 0, kind, (DATA_BASE, 0)),
        SC_SUCCESS
    );
    let data = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
    match kind {
        (IDENTIFY_CNS_CONTROLLER, _) => assert_eq!(&data[516..520], &2u32.to_le_bytes()),
        (IDENTIFY_CNS_NAMESPACE, NSID) => assert_eq!(&data[..8], &2048u64.to_le_bytes()),
        (IDENTIFY_CNS_NAMESPACE, NSID2) => assert_eq!(&data[..8], &3u64.to_le_bytes()),
        (IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST, 0) => {
            assert_eq!(&data[..8], &[1, 0, 0, 0, 2, 0, 0, 0])
        }
        (IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST, NSID) => {
            assert_eq!(&data[..8], &[2, 0, 0, 0, 0, 0, 0, 0])
        }
        (IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST, _) => assert_eq!(&data[..2], &[3, 16]),
        _ => assert!(data.iter().all(|byte| *byte == 0)),
    }
    data
}

pub(super) fn scattered(kind: (u32, u32)) {
    let data = reference(kind);
    for offset in [4, 512, PAGE_SIZE - 4] {
        let (mut ctrl, mut mem) = fixture();
        fill_data(&mut mem);
        let prp1 = DATA_BASE + offset as u64;
        assert_eq!(
            submit(&mut ctrl, &mut mem, 0, kind, (prp1, SECOND_PAGE)),
            SC_SUCCESS
        );
        let first_len = PAGE_SIZE - offset;
        let mut expected = vec![SENTINEL; DATA_LEN];
        expected[offset..PAGE_SIZE].copy_from_slice(&data[..first_len]);
        expected[4 * PAGE_SIZE..4 * PAGE_SIZE + offset].copy_from_slice(&data[first_len..]);
        let actual = mem.read_bytes(DATA_BASE, DATA_LEN).unwrap();
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(
                actual, expected,
                "CNS/NSID={kind:?}, PRP1 offset={offset}, data-region byte={index}"
            );
        }
    }
}
