//! A restored controller and the APST admin payload keep honouring the guest's PRP layout.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

const PAGES: usize = 3;

/// One three-page transfer described by a PRP list, issued after a snapshot/restore round
/// trip. Returns the completion status and the (actual, expected) bytes it moved.
fn restored_list_transfer(write: bool, direct: bool) -> (u16, bool) {
    let disk: Vec<u8> = (0..PAGE_SIZE * PAGES).map(|i| (i % 251) as u8).collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x40000);
    if direct {
        mem.enable_host_ptr();
    }
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
    let snapshot = ctrl.snapshot_state();
    ctrl.restore_state(&snapshot);
    let pages: Vec<u64> = (0..PAGES as u64)
        .map(|i| DATA_BASE + 0x2000 + i * 2 * PAGE_SIZE_U64)
        .collect();
    let list: Vec<u8> = pages[1..]
        .iter()
        .flat_map(|page| page.to_le_bytes())
        .collect();
    assert!(mem.write_bytes(DATA_BASE, &list));
    let payload: Vec<u8> = (0..PAGE_SIZE * PAGES)
        .map(|i| (i % 13) as u8 ^ 0xa5)
        .collect();
    for (chunk, page) in payload.chunks(PAGE_SIZE).zip(&pages) {
        assert!(!write || mem.write_bytes(*page, chunk));
    }
    let op = if write { NVM_OP_WRITE } else { NVM_OP_READ };
    let blocks = (PAGE_SIZE * PAGES / LBA_SIZE) as u32;
    let sqe = encode_sqe_with_prps(op, 0x70, NSID, pages[0], DATA_BASE, [0, 0, blocks - 1]);
    let status = submit_io(&mut ctrl, &mut mem, 0, &sqe);
    let moved = if write {
        ctrl.disk_image()[..PAGE_SIZE * PAGES] == payload[..]
    } else {
        let guest: Vec<u8> = pages
            .iter()
            .flat_map(|page| mem.read_bytes(*page, PAGE_SIZE).unwrap())
            .collect();
        guest == disk
    };
    (status, moved)
}

#[test]
fn restored_controller_still_decodes_prp_lists() {
    let mut results = Vec::new();
    for write in [false, true] {
        for direct in [false, true] {
            let (status, moved) = restored_list_transfer(write, direct);
            results.push((write, direct, status, moved));
        }
    }
    let ok = |r: &(bool, bool, u16, bool)| r.2 == SC_SUCCESS && r.3;
    assert!(
        results.iter().all(ok),
        "(write, direct, status, data moved): {results:?}"
    );
}

#[test]
fn apst_feature_data_follows_prp2_across_a_page_boundary() {
    let (mut ctrl, mut mem) = enabled_controller();
    let (prp1, prp2) = (DATA_BASE + 0xf80, DATA_BASE + 0x2000);
    assert!(mem.write_bytes(prp1, &[0xcc; 0x80]));
    assert!(mem.write_bytes(DATA_BASE + 0x1000, &[0xaa; 0x80]));
    assert!(mem.write_bytes(prp2, &[0xbb; 0x80]));
    let fid = u32::from(FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION);
    let sqe = encode_sqe_with_prps(ADMIN_OP_GET_FEATURES, 9, 0, prp1, prp2, [fid, 0, 0]);
    submit_admin(&mut ctrl, &mut mem, 0, &sqe);
    let cqe = read_completion(&mem, ACQ_BASE, 0);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    assert_eq!(mem.read_bytes(prp1, 0x80).unwrap(), vec![0u8; 0x80]);
    let next_page = mem.read_bytes(DATA_BASE + 0x1000, 0x80).unwrap();
    assert_eq!(
        next_page,
        vec![0xaa; 0x80],
        "the page after PRP1 is not the guest's buffer"
    );
    let second = mem.read_bytes(prp2, 0x80).unwrap();
    assert_eq!(
        second,
        vec![0u8; 0x80],
        "the rest of the table belongs at PRP2"
    );
}
