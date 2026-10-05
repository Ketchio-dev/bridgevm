//! CREATE must not replace a live queue or attach I/O to the admin CQ.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;

fn admin(ctrl: &mut NvmeController, mem: &mut FakeMem, entry: &[u8; 64]) -> u16 {
    let sq_slot = ctrl.sqs[0].as_ref().unwrap().head;
    let cq_slot = ctrl.cqs[0].as_ref().unwrap().tail;
    assert!(mem.write_bytes(ASQ_BASE + u64::from(sq_slot) * SQ_ENTRY_SIZE, entry));
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, u64::from((sq_slot + 1) % QDEPTH));
    ctrl.process(mem);
    let completion = read_completion(mem, ACQ_BASE, cq_slot);
    assert_eq!(
        u16::from_le_bytes([completion[12], completion[13]]),
        u16::from_le_bytes([entry[2], entry[3]])
    );
    let head = ctrl.cqs[0].as_ref().unwrap().tail;
    ctrl.mmio_write(REG_DOORBELL_BASE + 4, 4, u64::from(head));
    completion_status(&completion)
}

fn create_cq(base: u64) -> [u8; 64] {
    encode_sqe(
        ADMIN_OP_CREATE_IO_CQ,
        1,
        0,
        base,
        (1 << 16) | 1,
        CREATE_IO_CQ_PC_BIT,
        0,
    )
}

fn create_sq(base: u64, cqid: u16) -> [u8; 64] {
    encode_sqe(
        ADMIN_OP_CREATE_IO_SQ,
        2,
        0,
        base,
        (7 << 16) | 1,
        (u32::from(cqid) << 16) | 1,
        0,
    )
}

fn enabled_for_io() -> (NvmeController, FakeMem) {
    let mut ctrl = NvmeController::new(1 << 20);
    let mut mem = FakeMem::new(MEM_BASE, 0xa000);
    ctrl.mmio_write(
        REG_AQA,
        4,
        (u64::from(QDEPTH - 1) << 16) | u64::from(QDEPTH - 1),
    );
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    // Select 64-byte SQEs and 16-byte CQEs when enabling the controller.
    ctrl.mmio_write(REG_CC, 4, (6 << 16) | (4 << 20) | 1);
    // Negotiate one I/O SQ and one I/O CQ before creating either queue.
    assert_eq!(
        admin(
            &mut ctrl,
            &mut mem,
            &encode_sqe(
                ADMIN_OP_SET_FEATURES,
                0,
                0,
                0,
                u32::from(FEATURE_NUMBER_OF_QUEUES),
                0,
                0,
            )
        ),
        SC_SUCCESS
    );
    (ctrl, mem)
}

fn blocked_write() -> (NvmeController, FakeMem) {
    let (mut ctrl, mut mem) = enabled_for_io();
    assert_eq!(
        admin(&mut ctrl, &mut mem, &create_cq(IO_CQ_BASE)),
        SC_SUCCESS
    );
    assert_eq!(
        admin(&mut ctrl, &mut mem, &create_sq(IO_SQ_BASE, 1)),
        SC_SUCCESS
    );
    for slot in 0..2 {
        let data = DATA_BASE + u64::from(slot) * PAGE_SIZE_U64;
        assert!(mem.write_bytes(data, &vec![0x61 + slot as u8; LBA_SIZE]));
        assert!(mem.write_bytes(
            IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE,
            &encode_sqe(NVM_OP_WRITE, 10 + slot, NSID, data, u32::from(slot), 0, 0)
        ));
    }
    ctrl.mmio_write(REG_DOORBELL_BASE + 8, 4, 2);
    ctrl.process(&mut mem);
    assert_eq!(ctrl.sqs[1].as_ref().unwrap().head, 1);
    assert_eq!(ctrl.cqs[1].as_ref().unwrap().tail, 1);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
        &[0x61; LBA_SIZE]
    );
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0; LBA_SIZE]
    );
    (ctrl, mem)
}

fn consume_old_completion(ctrl: &mut NvmeController, mem: &mut FakeMem) {
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 1);
    ctrl.process(mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE],
        &[0x62; LBA_SIZE]
    );
    let entry = read_completion(mem, IO_CQ_BASE, 1);
    assert_eq!(u16::from_le_bytes([entry[12], entry[13]]), 11);
    assert_eq!(completion_status(&entry), SC_SUCCESS);
}

#[test]
fn duplicate_cq_preserves_backpressure_and_unread_completion() {
    let (mut ctrl, mut mem) = blocked_write();
    let unread = mem.read_bytes(IO_CQ_BASE, 32).unwrap();
    let replacement = MEM_BASE + 0x8000;
    let status = admin(&mut ctrl, &mut mem, &create_cq(replacement));
    let cq = ctrl.cqs[1].as_ref().unwrap();
    let changed_bytes = ctrl.disk.memory_image().unwrap()[LBA_SIZE..2 * LBA_SIZE]
        .iter()
        .filter(|&&b| b != 0)
        .count();
    assert_eq!((status, cq.base, changed_bytes), (0x0101, IO_CQ_BASE, 0),
        "duplicate CQ creation must reject the ID before replacing the live ring or releasing blocked disk I/O");
    assert_eq!((cq.head, cq.tail, cq.phase), (0, 1, true));
    assert_eq!(mem.read_bytes(IO_CQ_BASE, 32).unwrap(), unread);
    assert_eq!(mem.read_bytes(replacement, 32).unwrap(), [0; 32]);
    consume_old_completion(&mut ctrl, &mut mem);
}

#[test]
fn duplicate_sq_preserves_original_pending_write() {
    let (mut ctrl, mut mem) = blocked_write();
    let status = admin(&mut ctrl, &mut mem, &create_sq(MEM_BASE + 0x7000, 1));
    let sq = ctrl.sqs[1].as_ref().unwrap();
    assert_eq!(
        (
            status,
            sq.base,
            sq.head,
            sq.tail_doorbell,
            ctrl.sq_has_work(1)
        ),
        (0x0101, IO_SQ_BASE, 1, 2, true),
        "duplicate SQ creation must reject the ID and preserve pending work"
    );
    consume_old_completion(&mut ctrl, &mut mem);
}

#[test]
fn io_sq_cannot_target_the_admin_completion_queue() {
    let (mut ctrl, mut mem) = enabled_for_io();
    let status = admin(&mut ctrl, &mut mem, &create_sq(IO_SQ_BASE, 0));
    assert_eq!(
        (status, ctrl.sqs.get(1).is_some_and(Option::is_some)),
        (0x0101, false),
        "an I/O SQ must not be installed with admin CQ0 as its completion queue"
    );
}
