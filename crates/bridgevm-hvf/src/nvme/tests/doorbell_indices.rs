//! Finite doorbell-state tests: malformed tails must never reach the drain loop.

use crate::nvme::*;

#[test]
fn doorbell_rejects_admin_tail_equal_to_queue_depth() {
    let mut ctrl = NvmeController::new(4096);
    ctrl.mmio_write(REG_AQA, 4, 1 | (1 << 16));
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 2);

    let sq = ctrl.sqs[0].as_ref().unwrap();
    assert_eq!(sq.size, 2);
    assert_eq!((sq.tail_doorbell, ctrl.sq_has_work(0)), (0, false));
    assert_eq!(ctrl.pending_sq_bits, vec![0]);
}

fn controller_with_queue_pair(qid: usize) -> NvmeController {
    let mut ctrl = NvmeController::new(4096);
    // Deliberately different depths: each doorbell must use its own queue size.
    ctrl.mmio_write(REG_AQA, 4, 1 | (2 << 16));
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    if qid != 0 {
        let sq = ctrl.sqs[0].clone();
        let cq = ctrl.cqs[0].clone();
        ctrl.sqs.resize(qid + 1, None);
        ctrl.cqs.resize(qid + 1, None);
        ctrl.sqs[qid] = sq;
        ctrl.cqs[qid] = cq;
    }
    ctrl
}

#[test]
fn invalid_sq_doorbells_preserve_tail_and_pending_work() {
    for qid in [0, 1, usize::from(MAX_IO_QUEUE_PAIRS)] {
        for pending in [false, true] {
            let mut ctrl = controller_with_queue_pair(qid);
            let offset = REG_DOORBELL_BASE + qid as u64 * 8;
            if pending {
                ctrl.mmio_write(offset, 4, 1);
            }
            let previous_tail = ctrl.sqs[qid].as_ref().unwrap().tail_doorbell;
            let previous_pending = ctrl.pending_sq_bits.clone();
            for invalid in [2, 3, u64::from(u16::MAX), 0x1_0002] {
                ctrl.mmio_write(offset, 4, invalid);
                assert_eq!(ctrl.sqs[qid].as_ref().unwrap().tail_doorbell, previous_tail);
                assert_eq!(ctrl.sqs[qid].as_ref().unwrap().head, 0);
                assert_eq!(ctrl.pending_sq_bits, previous_pending);
                assert_eq!(ctrl.sq_has_work(qid), pending);
            }
        }
    }
}

#[test]
fn invalid_cq_doorbells_preserve_head_and_submission_work() {
    for qid in [0, 1, usize::from(MAX_IO_QUEUE_PAIRS)] {
        let mut ctrl = controller_with_queue_pair(qid);
        let sq_offset = REG_DOORBELL_BASE + qid as u64 * 8;
        let cq_offset = sq_offset + 4;
        ctrl.mmio_write(sq_offset, 4, 1);
        ctrl.mmio_write(cq_offset, 4, 2);
        let previous_pending = ctrl.pending_sq_bits.clone();
        for invalid in [3, 4, u64::from(u16::MAX), 0x1_0003] {
            ctrl.mmio_write(cq_offset, 4, invalid);
            let cq = ctrl.cqs[qid].as_ref().unwrap();
            assert_eq!((cq.head, cq.tail, cq.phase), (2, 0, true));
            assert_eq!(ctrl.sqs[qid].as_ref().unwrap().tail_doorbell, 1);
            assert_eq!(ctrl.pending_sq_bits, previous_pending);
        }
    }
}

#[test]
fn valid_doorbells_preserve_wrap_masking_and_pending_semantics() {
    for qid in [0, 1, usize::from(MAX_IO_QUEUE_PAIRS)] {
        let mut ctrl = controller_with_queue_pair(qid);
        let sq_offset = REG_DOORBELL_BASE + qid as u64 * 8;
        let cq_offset = sq_offset + 4;
        ctrl.mmio_write(sq_offset, 4, 0xffff_0001);
        assert_eq!(ctrl.sqs[qid].as_ref().unwrap().tail_doorbell, 1);
        assert_eq!(ctrl.pending_sq_bits, vec![1u64 << qid]);

        // Model a consumed entry without entering the queue drain.
        ctrl.sqs[qid].as_mut().unwrap().head = 1;
        ctrl.mmio_write(sq_offset, 4, 0);
        assert_eq!(ctrl.sqs[qid].as_ref().unwrap().tail_doorbell, 0);
        assert_eq!(ctrl.pending_sq_bits, vec![1u64 << qid]);
        ctrl.mmio_write(sq_offset, 4, 1);
        assert_eq!(ctrl.pending_sq_bits, vec![0]);

        ctrl.mmio_write(cq_offset, 4, 0xffff_0002);
        assert_eq!(ctrl.cqs[qid].as_ref().unwrap().head, 2);
        ctrl.mmio_write(cq_offset, 4, 0);
        assert_eq!(ctrl.cqs[qid].as_ref().unwrap().head, 0);
        assert_eq!(ctrl.pending_sq_bits, vec![0]);
    }
}

#[test]
fn absent_unaligned_and_outside_doorbells_do_not_change_queue_state() {
    let mut ctrl = controller_with_queue_pair(0);
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);
    for offset in [
        REG_DOORBELL_BASE + 1,
        REG_DOORBELL_BASE + 2,
        REG_DOORBELL_BASE + 8,
        REG_DOORBELL_BASE + 12,
        REG_DOORBELL_END,
        REG_DOORBELL_END + 4,
    ] {
        ctrl.mmio_write(offset, 4, 0);
        assert_eq!(ctrl.sqs.len(), 1);
        assert_eq!(ctrl.cqs.len(), 1);
        assert_eq!(ctrl.sqs[0].as_ref().unwrap().tail_doorbell, 1);
        assert_eq!(ctrl.cqs[0].as_ref().unwrap().head, 0);
        assert_eq!(ctrl.pending_sq_bits, vec![1]);
    }
}
