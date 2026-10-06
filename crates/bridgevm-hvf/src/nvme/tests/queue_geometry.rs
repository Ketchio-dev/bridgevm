//! Queue geometry is validated before publishing a usable queue identity.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

const SQ: u64 = MEM_BASE + 0x10000;
const CQ: u64 = MEM_BASE + 0x20000;
const DATA: u64 = MEM_BASE + 0x30000;
const INVALID_SIZE: u16 = 0x0102;

fn admin(ctrl: &mut NvmeController, mem: &mut FakeMem, entry: &[u8; 64]) -> u16 {
    let sq = ctrl.sqs[0].as_ref().unwrap().head;
    let cq = ctrl.cqs[0].as_ref().unwrap().tail;
    assert!(mem.write_bytes(ASQ_BASE + u64::from(sq) * SQ_ENTRY_SIZE, entry));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, u64::from((sq + 1) % QDEPTH));
    ctrl.process(mem);
    let completion = read_completion(mem, ACQ_BASE, cq);
    assert_eq!(&completion[12..14], &entry[2..4]);
    let tail = ctrl.cqs[0].as_ref().unwrap().tail;
    ctrl.mmio_write(REG_DOORBELL_BASE + 4, 4, u64::from(tail));
    completion_status(&completion)
}

fn enabled() -> (NvmeController, FakeMem) {
    let mut ctrl = NvmeController::new(1 << 20);
    let mut mem = FakeMem::new(MEM_BASE, 0x40000);
    ctrl.mmio_write(
        REG_AQA,
        4,
        (u64::from(QDEPTH - 1) << 16) | u64::from(QDEPTH - 1),
    );
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    ctrl.mmio_write(REG_CC, 4, (6 << 16) | (4 << 20) | 1);
    assert_eq!(ctrl.mmio_read(REG_CAP, 8) & CAP_CQR_BIT, CAP_CQR_BIT);
    assert_eq!(
        ctrl.mmio_read(REG_CAP, 8) & 0xffff,
        u64::from(MAX_QUEUE_ENTRIES - 1)
    );
    let entry = encode_sqe(
        ADMIN_OP_SET_FEATURES,
        1,
        0,
        0,
        u32::from(FEATURE_NUMBER_OF_QUEUES),
        0,
        0,
    );
    assert_eq!(admin(&mut ctrl, &mut mem, &entry), SC_SUCCESS);
    (ctrl, mem)
}

fn create(op: u8, qsize: u16, pc: bool) -> [u8; 64] {
    let (base, cqid) = if op == ADMIN_OP_CREATE_IO_SQ {
        (SQ, 1 << 16)
    } else {
        (CQ, 0)
    };
    encode_sqe(
        op,
        2,
        0,
        base,
        (u32::from(qsize) << 16) | 1,
        cqid | u32::from(pc),
        0,
    )
}

fn round_trip(ctrl: &mut NvmeController, mem: &mut FakeMem) {
    // More than two commands exercise both wrap and phase changes at minimum depth.
    for cid in 10..16 {
        let write = cid % 2 == 0;
        assert!(mem.write_bytes(DATA, &[if write { 0x5a } else { 0 }; LBA_SIZE]));
        let sq = ctrl.sqs[1].as_ref().unwrap();
        let (slot, tail) = (sq.head, (sq.head + 1) % sq.size);
        let cq = ctrl.cqs[1].as_ref().unwrap();
        let (cq_slot, phase) = (cq.tail, cq.phase);
        let op = if write { NVM_OP_WRITE } else { NVM_OP_READ };
        assert!(mem.write_bytes(
            SQ + u64::from(slot) * SQ_ENTRY_SIZE,
            &encode_sqe(op, cid, NSID, DATA, 0, 0, 0)
        ));
        ctrl.mmio_write(REG_DOORBELL_BASE + 8, 4, u64::from(tail));
        ctrl.process(mem);
        let completion = read_completion(mem, CQ, cq_slot);
        assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), cid);
        assert_eq!(completion_status(&completion), SC_SUCCESS);
        assert_eq!(completion[14] & 1, u8::from(phase));
        assert_eq!(mem.read_bytes(DATA, LBA_SIZE).unwrap(), [0x5a; LBA_SIZE]);
        assert_eq!(
            &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
            &[0x5a; LBA_SIZE]
        );
        let cq_tail = ctrl.cqs[1].as_ref().unwrap().tail;
        ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, u64::from(cq_tail));
    }
}

fn rejected_then_retry(op: u8, qsize: u16, pc: bool, expected: u16) {
    let (mut ctrl, mut mem) = enabled();
    if op == ADMIN_OP_CREATE_IO_SQ {
        assert_eq!(
            admin(&mut ctrl, &mut mem, &create(ADMIN_OP_CREATE_IO_CQ, 1, true)),
            SC_SUCCESS
        );
    }
    let status = admin(&mut ctrl, &mut mem, &create(op, qsize, pc));
    let installed = if op == ADMIN_OP_CREATE_IO_SQ {
        ctrl.sqs.get(1).is_some_and(Option::is_some)
    } else {
        ctrl.cqs.get(1).is_some_and(Option::is_some)
    };
    assert_eq!(
        (status, installed),
        (expected, false),
        "invalid geometry must not reserve the queue ID"
    );
    assert_eq!(mem.read_bytes(SQ, 64).unwrap(), [0; 64]);
    assert_eq!(mem.read_bytes(CQ, 32).unwrap(), [0; 32]);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
        &[0; LBA_SIZE]
    );
    assert_eq!(admin(&mut ctrl, &mut mem, &create(op, 1, true)), SC_SUCCESS);
    if op == ADMIN_OP_CREATE_IO_CQ {
        assert_eq!(
            admin(&mut ctrl, &mut mem, &create(ADMIN_OP_CREATE_IO_SQ, 1, true)),
            SC_SUCCESS
        );
    }
    round_trip(&mut ctrl, &mut mem);
}

#[test]
fn sq_one_slot_is_rejected_without_reserving_id() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_SQ, 0, true, INVALID_SIZE);
}

#[test]
fn sq_above_advertised_maximum_reports_invalid_size() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_SQ, MAX_QUEUE_ENTRIES, true, INVALID_SIZE);
}

#[test]
fn cq_one_slot_reports_invalid_size() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_CQ, 0, true, INVALID_SIZE);
}

#[test]
fn cq_above_advertised_maximum_reports_invalid_size() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_CQ, MAX_QUEUE_ENTRIES, true, INVALID_SIZE);
}

#[test]
fn sq_noncontiguous_is_rejected_without_reserving_id() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_SQ, 7, false, SC_INVALID_FIELD);
}

#[test]
fn cq_noncontiguous_still_rejects_and_allows_retry() {
    rejected_then_retry(ADMIN_OP_CREATE_IO_CQ, 7, false, SC_INVALID_FIELD);
}

fn valid(qsize: u16) {
    let (mut ctrl, mut mem) = enabled();
    for op in [ADMIN_OP_CREATE_IO_CQ, ADMIN_OP_CREATE_IO_SQ] {
        assert_eq!(
            admin(&mut ctrl, &mut mem, &create(op, qsize, true)),
            SC_SUCCESS
        );
    }
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().size, qsize + 1);
    assert_eq!(ctrl.cqs[1].as_ref().unwrap().size, qsize + 1);
    round_trip(&mut ctrl, &mut mem);
}

#[test]
fn minimum_two_slot_queues_wrap_with_read_write_completions() {
    valid(1);
}

#[test]
fn advertised_maximum_queues_complete_read_write() {
    valid(MAX_QUEUE_ENTRIES - 1);
}
