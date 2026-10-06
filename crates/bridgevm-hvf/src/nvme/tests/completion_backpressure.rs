//! Mapped queues must retain unread completions and defer their associated I/O.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

fn admin(ctrl: &mut NvmeController, mem: &mut FakeMem, entry: &[u8; 64]) {
    let slot = ctrl.sqs[0].as_ref().unwrap().head;
    submit_admin(ctrl, mem, slot, entry);
    assert_eq!(
        completion_status(&read_completion(mem, ACQ_BASE, slot)),
        SC_SUCCESS
    );
    let head = ctrl.cqs[0].as_ref().unwrap().tail;
    ctrl.mmio_write(REG_DOORBELL_BASE + 4, 4, u64::from(head));
}

fn create_cq(ctrl: &mut NvmeController, mem: &mut FakeMem, qid: u16, base: u64, depth: u16) {
    admin(
        ctrl,
        mem,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            qid,
            0,
            base,
            (u32::from(depth - 1) << 16) | u32::from(qid),
            CREATE_IO_CQ_PC_BIT,
            0,
        ),
    );
}

fn create_sq(ctrl: &mut NvmeController, mem: &mut FakeMem, qid: u16, base: u64, cqid: u16) {
    admin(
        ctrl,
        mem,
        &encode_sqe(
            ADMIN_OP_CREATE_IO_SQ,
            qid,
            0,
            base,
            (u32::from(QDEPTH - 1) << 16) | u32::from(qid),
            u32::from(cqid) << 16,
            0,
        ),
    );
}

fn stage_write(mem: &mut FakeMem, sq_base: u64, slot: u16, lba: u16, cid: u16) {
    let data = DATA_BASE + u64::from(lba) * PAGE_SIZE_U64;
    assert!(mem.write_bytes(data, &vec![cid as u8; LBA_SIZE]));
    assert!(mem.write_bytes(
        sq_base + u64::from(slot) * SQ_ENTRY_SIZE,
        &encode_sqe(NVM_OP_WRITE, cid, NSID, data, u32::from(lba), 0, 0,)
    ));
}

fn ring_sq(ctrl: &mut NvmeController, qid: u16, tail: u16) {
    ctrl.mmio_write(REG_DOORBELL_BASE + u64::from(qid) * 8, 4, u64::from(tail));
}

fn consume(ctrl: &mut NvmeController, mem: &mut FakeMem, cqid: u16, head: u16) {
    ctrl.mmio_write(
        REG_DOORBELL_BASE + u64::from(cqid) * 8 + 4,
        4,
        u64::from(head),
    );
    ctrl.process(mem);
}

fn assert_lba(ctrl: &NvmeController, lba: usize, byte: u8) {
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[lba * LBA_SIZE..(lba + 1) * LBA_SIZE],
        vec![byte; LBA_SIZE]
    );
}

fn assert_cid(mem: &FakeMem, cq_base: u64, slot: u16, cid: u16, phase: bool) {
    let entry = read_completion(mem, cq_base, slot);
    assert_eq!(u16::from_le_bytes([entry[12], entry[13]]), cid);
    assert_eq!(completion_status(&entry), SC_SUCCESS);
    assert_eq!(entry[14] & 1, u8::from(phase));
}

#[test]
fn full_cq_preserves_unread_entries_and_defers_disk_writes_until_consumed() {
    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x14000);
    create_cq(&mut ctrl, &mut mem, 1, IO_CQ_BASE, 3);
    create_sq(&mut ctrl, &mut mem, 1, IO_SQ_BASE, 1);
    for slot in 0..4 {
        stage_write(&mut mem, IO_SQ_BASE, slot, slot, 11 + slot);
    }
    ring_sq(&mut ctrl, 1, 4);
    ctrl.process(&mut mem);

    assert_cid(&mem, IO_CQ_BASE, 0, 11, true);
    assert_cid(&mem, IO_CQ_BASE, 1, 12, true);
    assert_eq!(read_completion(&mem, IO_CQ_BASE, 2), [0; 16]);
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 2);
    assert!(ctrl.sq_has_work(1));
    assert_lba(&ctrl, 0, 11);
    assert_lba(&ctrl, 1, 12);
    assert_lba(&ctrl, 2, 0);
    assert_lba(&ctrl, 3, 0);
    let unread = mem.read_bytes(IO_CQ_BASE, 48).unwrap();
    ctrl.process(&mut mem);
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 48).unwrap(), unread);
    assert_lba(&ctrl, 2, 0);

    consume(&mut ctrl, &mut mem, 1, 1);
    assert_cid(&mem, IO_CQ_BASE, 1, 12, true);
    assert_cid(&mem, IO_CQ_BASE, 2, 13, true);
    assert_lba(&ctrl, 2, 13);
    assert_lba(&ctrl, 3, 0);
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 3);
    consume(&mut ctrl, &mut mem, 1, 2);
    assert_cid(&mem, IO_CQ_BASE, 0, 14, false);
    assert_cid(&mem, IO_CQ_BASE, 2, 13, true);
    assert_lba(&ctrl, 3, 14);
    assert!(!ctrl.sq_has_work(1));
    let cids: Vec<_> = ctrl
        .recent_command_trace()
        .iter()
        .filter(|entry| entry.sqid == 1)
        .map(|entry| entry.command_id)
        .collect();
    assert_eq!(cids, [11, 12, 13, 14]);
}

#[test]
fn shared_cq_applies_backpressure_to_each_submission_queue() {
    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x14000);
    let second_sq = MEM_BASE + 0x9000;
    create_cq(&mut ctrl, &mut mem, 1, IO_CQ_BASE, 3);
    create_sq(&mut ctrl, &mut mem, 1, IO_SQ_BASE, 1);
    create_sq(&mut ctrl, &mut mem, 2, second_sq, 1);
    for slot in 0..2 {
        stage_write(&mut mem, IO_SQ_BASE, slot, slot, 21 + slot);
        stage_write(&mut mem, second_sq, slot, slot + 2, 23 + slot);
    }
    ring_sq(&mut ctrl, 1, 2);
    ring_sq(&mut ctrl, 2, 2);
    ctrl.process(&mut mem);

    assert_cid(&mem, IO_CQ_BASE, 0, 21, true);
    assert_cid(&mem, IO_CQ_BASE, 1, 22, true);
    assert_eq!(ctrl.sqs[2].as_ref().unwrap().head, 0);
    assert!(ctrl.sq_has_work(2));
    assert_lba(&ctrl, 2, 0);
    assert_lba(&ctrl, 3, 0);
    consume(&mut ctrl, &mut mem, 1, 1);
    assert_cid(&mem, IO_CQ_BASE, 2, 23, true);
    assert_lba(&ctrl, 2, 23);
    assert_lba(&ctrl, 3, 0);
    consume(&mut ctrl, &mut mem, 1, 2);
    assert_cid(&mem, IO_CQ_BASE, 0, 24, false);
    assert_lba(&ctrl, 3, 24);
    assert!(!ctrl.sq_has_work(2));
}

#[test]
fn full_cq_does_not_stall_submissions_targeting_another_cq() {
    let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x14000);
    let second_sq = MEM_BASE + 0x9000;
    let second_cq = MEM_BASE + 0xa000;
    create_cq(&mut ctrl, &mut mem, 1, IO_CQ_BASE, 2);
    create_cq(&mut ctrl, &mut mem, 2, second_cq, 2);
    create_sq(&mut ctrl, &mut mem, 1, IO_SQ_BASE, 1);
    create_sq(&mut ctrl, &mut mem, 2, second_sq, 2);
    stage_write(&mut mem, IO_SQ_BASE, 0, 0, 31);
    stage_write(&mut mem, IO_SQ_BASE, 1, 1, 32);
    stage_write(&mut mem, second_sq, 0, 2, 33);
    ring_sq(&mut ctrl, 1, 2);
    ring_sq(&mut ctrl, 2, 1);
    ctrl.process(&mut mem);

    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 1);
    assert_lba(&ctrl, 1, 0);
    assert_cid(&mem, second_cq, 0, 33, true);
    assert_lba(&ctrl, 2, 33);
    assert!(!ctrl.sq_has_work(2));
    consume(&mut ctrl, &mut mem, 1, 1);
    assert_cid(&mem, IO_CQ_BASE, 1, 32, true);
    assert_lba(&ctrl, 1, 32);
    assert!(!ctrl.sq_has_work(1));
}

#[test]
fn create_cq_rejects_one_entry_without_installing_or_replacing_a_queue() {
    for occupied in [false, true] {
        let (mut ctrl, mut mem) = enabled_controller();
        if occupied {
            create_cq(&mut ctrl, &mut mem, 1, IO_CQ_BASE, 3);
        }
        let before = ctrl.snapshot_state();
        let entry = encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            MEM_BASE + 0xa000,
            1,
            CREATE_IO_CQ_PC_BIT,
            0,
        );
        assert_eq!(
            ctrl.admin_create_io_cq(&SubmissionEntry::from_bytes(&entry)),
            SC_INVALID_FIELD
        );
        assert_eq!(ctrl.snapshot_state(), before);
    }
}
