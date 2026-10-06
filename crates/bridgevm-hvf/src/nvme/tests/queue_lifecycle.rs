//! A successful queue deletion must stop subsequent access to that queue.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

fn admin(ctrl: &mut NvmeController, mem: &mut FakeMem, entry: &[u8; 64]) -> u16 {
    let sq_slot = ctrl.sqs[0].as_ref().unwrap().head;
    let cq_slot = ctrl.cqs[0].as_ref().unwrap().tail;
    assert!(mem.write_bytes(ASQ_BASE + u64::from(sq_slot) * SQ_ENTRY_SIZE, entry));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, u64::from((sq_slot + 1) % QDEPTH));
    ctrl.process(mem);
    let status = completion_status(&read_completion(mem, ACQ_BASE, cq_slot));
    let head = ctrl.cqs[0].as_ref().unwrap().tail;
    ctrl.mmio_write(REG_DOORBELL_BASE + 4, 4, u64::from(head));
    status
}

fn blocked_write() -> (NvmeController, FakeMem) {
    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0xa000);
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                1,
                0,
                IO_CQ_BASE,
                (1 << 16) | 1,
                CREATE_IO_CQ_PC_BIT,
                0,
            )
        ),
        SC_SUCCESS
    );
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                2,
                0,
                IO_SQ_BASE,
                (u32::from(QDEPTH - 1) << 16) | 1,
                (1 << 16) | 1,
                0,
            )
        ),
        SC_SUCCESS
    );
    // The depth-two CQ permits one completion; the second write stays in SQ1.
    for slot in 0..2 {
        let data = DATA_BASE + u64::from(slot) * PAGE_SIZE_U64;
        assert!(mem.write_bytes(data, &vec![0x71 + slot as u8; LBA_SIZE]));
        assert!(mem.write_bytes(
            IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE,
            &encode_sqe(NVM_OP_WRITE, 10 + slot, NSID, data, u32::from(slot), 0, 0)
        ));
    }
    ctrl.mmio_write(REG_DOORBELL_BASE + 8, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 1);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
        &[0x71; LBA_SIZE]
    );
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE]
    );
    (ctrl, mem)
}

#[test]
fn deleted_sq_does_not_resume_blocked_write_after_cq_consumption() {
    let (mut ctrl, mut mem) = blocked_write();
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(ADMIN_OP_DELETE_IO_SQ, 3, 0, 0, 1, 0, 0,)
        ),
        SC_SUCCESS
    );
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE]
    );
    let cq_before = mem.read_bytes(IO_CQ_BASE, 32).unwrap();
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE],
        "a command from a successfully deleted SQ executed after the delete completion"
    );
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), cq_before);
    assert!(ctrl.sqs[1].is_none());
    assert!(!ctrl.sq_has_work(1));
}

#[test]
fn controller_disable_discards_blocked_write_before_reenable() {
    let (mut ctrl, mut mem) = blocked_write();
    ctrl.mmio_write(REG_CC, 4, 0);
    ctrl.mmio_write(REG_CC, 4, 1);
    let cq_before = mem.read_bytes(IO_CQ_BASE, 32).unwrap();
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE]
    );
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), cq_before);
    assert!(!ctrl.sq_has_work(1));
}

#[test]
fn attached_cq_delete_is_rejected_and_preserves_blocked_traffic() {
    let (mut ctrl, mut mem) = blocked_write();
    let cq_before = mem.read_bytes(IO_CQ_BASE, 32).unwrap();
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(ADMIN_OP_DELETE_IO_CQ, 3, 0, 0, 1, 0, 0,)
        ),
        0x010c,
        "expected command-specific Invalid Queue Deletion"
    );
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), cq_before);
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 1);
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0x72; LBA_SIZE]
    );
    let completion = read_completion(&mem, IO_CQ_BASE, 1);
    assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), 11);
    assert_eq!(completion_status(&completion), SC_SUCCESS);
}

#[test]
fn delete_rejects_admin_missing_out_of_range_and_already_deleted_queue_ids() {
    for opcode in [ADMIN_OP_DELETE_IO_SQ, ADMIN_OP_DELETE_IO_CQ] {
        let (mut ctrl, mut mem) = enabled_controller();
        for qid in [0, 1, MAX_IO_QUEUE_PAIRS + 1, u16::MAX] {
            assert_eq!(
                admin(
                    &mut ctrl,
                    &mut mem,
                    &encode_sqe(opcode, qid, 0, 0, u32::from(qid), 0, 0,)
                ),
                0x0101,
                "expected command-specific Invalid Queue Identifier"
            );
            assert!(ctrl.sqs[0].is_some());
            assert!(ctrl.cqs[0].is_some());
        }
    }
    let (mut ctrl, mut mem) = blocked_write();
    for opcode in [ADMIN_OP_DELETE_IO_SQ, ADMIN_OP_DELETE_IO_CQ] {
        assert_eq!(
            admin(&mut ctrl, &mut mem, &encode_sqe(opcode, 3, 0, 0, 1, 0, 0)),
            SC_SUCCESS
        );
        assert_eq!(
            admin(&mut ctrl, &mut mem, &encode_sqe(opcode, 4, 0, 0, 1, 0, 0)),
            0x0101
        );
    }
}

#[test]
fn deleted_queue_pair_recreates_at_new_addresses_without_old_work() {
    let (mut ctrl, mut mem) = blocked_write();
    let old_cq = mem.read_bytes(IO_CQ_BASE, 32).unwrap();
    for opcode in [ADMIN_OP_DELETE_IO_SQ, ADMIN_OP_DELETE_IO_CQ] {
        assert_eq!(
            admin(&mut ctrl, &mut mem, &encode_sqe(opcode, 3, 0, 0, 1, 0, 0)),
            SC_SUCCESS
        );
    }
    assert!(ctrl.sqs[1].is_none());
    assert!(ctrl.cqs[1].is_none());
    let new_sq = MEM_BASE + 0x7000;
    let new_cq = MEM_BASE + 0x8000;
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                4,
                0,
                new_cq,
                (1 << 16) | 1,
                CREATE_IO_CQ_PC_BIT,
                0,
            )
        ),
        SC_SUCCESS
    );
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                5,
                0,
                new_sq,
                (1 << 16) | 1,
                (1 << 16) | 1,
                0,
            )
        ),
        SC_SUCCESS
    );
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE]
    );
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), old_cq);
    assert_eq!(read_completion(&mem, new_cq, 0), [0; 16]);
    assert!(mem.write_bytes(
        new_sq,
        &encode_sqe(NVM_OP_WRITE, 42, NSID, DATA_BASE, 2, 0, 0)
    ));
    ctrl.mmio_write(REG_DOORBELL_BASE + 8, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[2 * LBA_SIZE..3 * LBA_SIZE],
        &[0x71; LBA_SIZE]
    );
    let completion = read_completion(&mem, new_cq, 0);
    assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), 42);
    assert_eq!(completion_status(&completion), SC_SUCCESS);
    assert_eq!(completion[14] & 1, 1);
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), old_cq);
}
