//! Number-of-Queues completion consistency through admin SQ/MMIO/CQ paths.

use super::super::*;
use super::helpers::*;

fn feature(ctrl: &mut NvmeController, mem: &mut FakeMem, slot: u16, op: u8, request: u32) -> u32 {
    let command = encode_sqe(
        op,
        0x40 + slot,
        0,
        0,
        u32::from(FEATURE_NUMBER_OF_QUEUES),
        request,
        0,
    );
    submit_admin(ctrl, mem, slot, &command);
    let completion = read_completion(mem, ACQ_BASE, slot);
    assert_eq!(completion_status(&completion), SC_SUCCESS);
    assert_eq!(
        u16::from_le_bytes([completion[12], completion[13]]),
        0x40 + slot
    );
    completion_dw0(&completion)
}

fn set_and_get(request: u32) {
    let (mut ctrl, mut mem) = enabled_controller();
    let allocated = feature(&mut ctrl, &mut mem, 0, ADMIN_OP_SET_FEATURES, request);
    let current = feature(&mut ctrl, &mut mem, 1, ADMIN_OP_GET_FEATURES, 0);
    assert_eq!(
        allocated, current,
        "SET and GET must describe the same allocated queues"
    );
}

#[test]
fn one_queue_request_matches_current_allocation() {
    set_and_get(0);
}

#[test]
fn asymmetric_queue_request_matches_current_allocation() {
    set_and_get((3 << 16) | 1);
}

#[test]
fn capacity_and_larger_valid_requests_match_current_allocation() {
    set_and_get((7 << 16) | 7);
    set_and_get(0xfffe_fffe);
}

#[test]
fn repeated_requests_cannot_change_allocation_between_resets() {
    let (mut ctrl, mut mem) = enabled_controller();
    let initial = feature(&mut ctrl, &mut mem, 0, ADMIN_OP_GET_FEATURES, 0);
    let first = feature(&mut ctrl, &mut mem, 1, ADMIN_OP_SET_FEATURES, 0);
    let second = feature(&mut ctrl, &mut mem, 2, ADMIN_OP_SET_FEATURES, 0xfffe_fffe);
    let current = feature(&mut ctrl, &mut mem, 3, ADMIN_OP_GET_FEATURES, 0);
    assert_eq!([initial, first, second, current], [initial; 4]);
}

#[test]
fn controller_reset_keeps_set_and_get_consistent() {
    let (mut ctrl, mut mem) = enabled_controller();
    feature(&mut ctrl, &mut mem, 0, ADMIN_OP_SET_FEATURES, 0xfffe_fffe);
    ctrl.mmio_write(REG_CC, 4, 0);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & u64::from(CSTS_RDY_BIT), 0);
    mem.bytes.fill(0);
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    let allocated = feature(&mut ctrl, &mut mem, 0, ADMIN_OP_SET_FEATURES, 0);
    let current = feature(&mut ctrl, &mut mem, 1, ADMIN_OP_GET_FEATURES, 0);
    assert_eq!(allocated, current);
}

#[test]
fn reported_allocation_matches_queue_creation_capacity() {
    let (mut ctrl, mut mem) = enabled_controller();
    let allocated = feature(&mut ctrl, &mut mem, 0, ADMIN_OP_GET_FEATURES, 0);
    let sq_count = (allocated & 0xffff) + 1;
    let cq_count = (allocated >> 16) + 1;
    assert_eq!((sq_count, cq_count), (8, 8));
    for (slot, op, base, qid, cdw11, expected) in [
        (
            1,
            ADMIN_OP_CREATE_IO_CQ,
            IO_CQ_BASE,
            cq_count,
            1,
            SC_SUCCESS,
        ),
        (
            2,
            ADMIN_OP_CREATE_IO_SQ,
            IO_SQ_BASE,
            sq_count,
            (cq_count << 16) | 1,
            SC_SUCCESS,
        ),
        (
            3,
            ADMIN_OP_CREATE_IO_CQ,
            DATA_BASE,
            cq_count + 1,
            1,
            SC_INVALID_FIELD,
        ),
        (
            4,
            ADMIN_OP_CREATE_IO_SQ,
            DATA_BASE,
            sq_count + 1,
            (cq_count << 16) | 1,
            SC_INVALID_FIELD,
        ),
    ] {
        let command = encode_sqe(
            op,
            slot,
            0,
            base,
            (u32::from(QDEPTH - 1) << 16) | qid,
            cdw11,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, slot, &command);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, slot)),
            expected
        );
    }
}
