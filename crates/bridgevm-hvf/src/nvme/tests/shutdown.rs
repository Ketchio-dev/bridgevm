//! Shutdown notification through guest registers, including backend sync failure.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::{fs, io, path::PathBuf};

const CC_READY: u64 = 0x0046_0001; // IOSQES=6, IOCQES=4, EN=1.
const SHST_MASK: u64 = 3 << 2;
const SHST_COMPLETE: u64 = 2 << 2;

fn configure(mut ctrl: NvmeController) -> (NvmeController, FakeMem) {
    let aqa = u64::from(QDEPTH - 1);
    ctrl.mmio_write(REG_AQA, 4, aqa | (aqa << 16));
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    ctrl.mmio_write(REG_CC, 4, CC_READY);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 1);
    (ctrl, FakeMem::new(MEM_BASE, 0x9000))
}

fn admin(ctrl: &mut NvmeController, mem: &mut FakeMem, slot: u16, cmd: &[u8; 64]) {
    submit_admin(ctrl, mem, slot, cmd);
    let cqe = read_completion(mem, ACQ_BASE, slot);
    assert_eq!(&cqe[12..14], &cmd[2..4]);
    assert_eq!(cqe[14] & 1, 1);
    assert_eq!(completion_status(&cqe), SC_SUCCESS);
    ctrl.mmio_write(REG_DOORBELL_BASE + 4, 4, u64::from(slot + 1));
    ctrl.process(mem);
}

fn identify(ctrl: &mut NvmeController, mem: &mut FakeMem, slot: u16, cid: u16) {
    admin(
        ctrl,
        mem,
        slot,
        &encode_sqe(ADMIN_OP_IDENTIFY, cid, 0, DATA_BASE, 1, 0, 0),
    );
}

fn create_queues(ctrl: &mut NvmeController, mem: &mut FakeMem) {
    for (slot, opcode, base, flags) in [
        (0, ADMIN_OP_CREATE_IO_CQ, IO_CQ_BASE, 1),
        (1, ADMIN_OP_CREATE_IO_SQ, IO_SQ_BASE, (1 << 16) | 1),
    ] {
        admin(
            ctrl,
            mem,
            slot,
            &encode_sqe(opcode, 10 + slot, 0, base, (7 << 16) | 1, flags, 0),
        );
    }
}

fn delete_queues(ctrl: &mut NvmeController, mem: &mut FakeMem) {
    for (slot, opcode) in [(2, ADMIN_OP_DELETE_IO_SQ), (3, ADMIN_OP_DELETE_IO_CQ)] {
        admin(
            ctrl,
            mem,
            slot,
            &encode_sqe(opcode, 10 + slot, 0, 0, 1, 0, 0),
        );
    }
}

fn notify(ctrl: &mut NvmeController, mem: &mut FakeMem, shn: u64) -> u64 {
    ctrl.mmio_write(REG_CC, 4, CC_READY | (shn << 14));
    ctrl.process(mem);
    ctrl.mmio_read(REG_CSTS, 4)
}

#[test]
fn normal_shutdown_completes_after_io_queue_deletion_and_restarts_after_reset() {
    let (mut ctrl, mut mem) = configure(NvmeController::new(LBA_SIZE * 8));
    create_queues(&mut ctrl, &mut mem);
    assert!(mem.write_bytes(DATA_BASE, &[0x79; LBA_SIZE]));
    let write = encode_sqe(NVM_OP_WRITE, 0x21, NSID, DATA_BASE, 0, 0, 0);
    assert_eq!(submit_io(&mut ctrl, &mut mem, 0, &write), SC_SUCCESS);
    assert_eq!(&read_completion(&mem, IO_CQ_BASE, 0)[12..14], &[0x21, 0]);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
        &[0x79; LBA_SIZE]
    );
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 1);
    delete_queues(&mut ctrl, &mut mem);
    identify(&mut ctrl, &mut mem, 4, 0x31);

    assert_eq!(notify(&mut ctrl, &mut mem, 1) & SHST_MASK, SHST_COMPLETE);
    assert_eq!(notify(&mut ctrl, &mut mem, 0) & SHST_MASK, SHST_COMPLETE);
    let snapshot = ctrl.snapshot_state();
    ctrl.restore_state(&snapshot);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & SHST_MASK, SHST_COMPLETE);
    ctrl.mmio_write(REG_CC, 4, CC_READY & !1);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 0);
    ctrl.mmio_write(REG_CC, 4, CC_READY);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 1);
    identify(&mut ctrl, &mut mem, 0, 0x32);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[..LBA_SIZE],
        &[0x79; LBA_SIZE]
    );
}

#[test]
fn abrupt_shutdown_completes_with_existing_quiescent_io_queues() {
    let (mut ctrl, mut mem) = configure(NvmeController::new(LBA_SIZE * 8));
    create_queues(&mut ctrl, &mut mem);
    let before = mem.bytes.clone();
    assert_eq!(notify(&mut ctrl, &mut mem, 2) & SHST_MASK, SHST_COMPLETE);
    assert_eq!(mem.bytes, before);
}

fn queue_blocked_write(ctrl: &mut NvmeController, mem: &mut FakeMem) {
    assert!(mem.write_bytes(DATA_BASE, &[0x59; LBA_SIZE]));
    // Seven unread completions fill the depth-eight CQ. An eighth WRITE is
    // submitted before shutdown, but cannot execute while that CQ is full.
    for slot in 0..7 {
        let write = encode_sqe(
            NVM_OP_WRITE,
            0x50 + slot,
            NSID,
            DATA_BASE,
            u32::from(slot),
            0,
            0,
        );
        assert_eq!(submit_io(ctrl, mem, slot, &write), SC_SUCCESS);
        assert_eq!(
            &read_completion(mem, IO_CQ_BASE, slot)[12..14],
            &(0x50 + slot).to_le_bytes()
        );
    }
    let blocked = encode_sqe(NVM_OP_WRITE, 0x57, NSID, DATA_BASE, 7, 0, 0);
    assert!(mem.write_bytes(IO_SQ_BASE + 7 * SQ_ENTRY_SIZE, &blocked));
    ctrl.mmio_write(REG_DOORBELL_BASE + 8, 4, 0);
    ctrl.process(mem);
    assert_eq!(
        &ctrl.disk.read_at((7 * LBA_SIZE) as u64, LBA_SIZE).unwrap()[..],
        &[0; LBA_SIZE]
    );
}

#[test]
fn abrupt_shutdown_does_not_resume_preexisting_backlog_after_snapshot_restore() {
    let (mut ctrl, mut mem) = configure(NvmeController::new(LBA_SIZE * 8));
    create_queues(&mut ctrl, &mut mem);
    queue_blocked_write(&mut ctrl, &mut mem);

    assert_eq!(notify(&mut ctrl, &mut mem, 2) & SHST_MASK, SHST_COMPLETE);
    assert_eq!(notify(&mut ctrl, &mut mem, 0) & SHST_MASK, SHST_COMPLETE);
    ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 7);
    let cq_before = mem
        .read_bytes(IO_CQ_BASE, usize::from(QDEPTH) * 16)
        .unwrap();
    let snapshot = ctrl.snapshot_state();
    ctrl.restore_state(&snapshot);
    ctrl.process(&mut mem);
    assert_eq!(
        &ctrl.disk.memory_image().unwrap()[7 * LBA_SIZE..],
        &[0; LBA_SIZE]
    );
    assert_eq!(
        mem.read_bytes(IO_CQ_BASE, usize::from(QDEPTH) * 16)
            .unwrap(),
        cq_before
    );
}

#[test]
fn normal_shutdown_can_clear_enable_in_the_notification_write() {
    let (mut ctrl, mut mem) = configure(NvmeController::new(LBA_SIZE * 8));
    identify(&mut ctrl, &mut mem, 0, 0x33);
    ctrl.mmio_write(REG_CC, 4, (CC_READY & !1) | (1 << 14));
    ctrl.process(&mut mem);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & (SHST_MASK | 1), SHST_COMPLETE);
    ctrl.mmio_write(REG_CC, 4, CC_READY);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 1);
    identify(&mut ctrl, &mut mem, 0, 0x34);
}

struct ScratchFiles([PathBuf; 2]);

impl ScratchFiles {
    fn new() -> Self {
        let paths = [
            temp_path("shutdown-primary"),
            temp_path("shutdown-secondary"),
        ];
        for path in &paths {
            fs::write(path, [0; LBA_SIZE * 8]).unwrap();
        }
        Self(paths)
    }

    fn controller(&self) -> (NvmeController, FakeMem) {
        let mut ctrl = NvmeController::with_raw_file(&self.0[0], true).unwrap();
        ctrl.attach_second_namespace_raw_file(&self.0[1], true)
            .unwrap();
        configure(ctrl)
    }
}

impl Drop for ScratchFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

#[test]
fn shutdown_syncs_both_namespaces_before_completion_and_does_not_repeat() {
    for shn in [1, 2] {
        let files = ScratchFiles::new();
        let (mut ctrl, mut mem) = files.controller();
        create_queues(&mut ctrl, &mut mem);
        for slot in 0..2 {
            assert!(mem.write_bytes(DATA_BASE, &[0x61 + slot as u8; LBA_SIZE]));
            let write = encode_sqe(
                NVM_OP_WRITE,
                0x60 + slot,
                u32::from(slot + 1),
                DATA_BASE,
                0,
                0,
                0,
            );
            assert_eq!(submit_io(&mut ctrl, &mut mem, slot, &write), SC_SUCCESS);
            assert_eq!(
                &read_completion(&mem, IO_CQ_BASE, slot)[12..14],
                &(0x60 + slot).to_le_bytes()
            );
        }
        ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 2);
        delete_queues(&mut ctrl, &mut mem);
        assert_eq!(notify(&mut ctrl, &mut mem, shn) & SHST_MASK, SHST_COMPLETE);
        assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
        assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);
        assert_eq!(notify(&mut ctrl, &mut mem, shn) & SHST_MASK, SHST_COMPLETE);
        assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
        assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);
    }
}

#[test]
fn failed_namespace_sync_never_reports_shutdown_complete() {
    for (shn, failed_nsid) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
        let files = ScratchFiles::new();
        let (mut ctrl, mut mem) = files.controller();
        if shn == 2 {
            create_queues(&mut ctrl, &mut mem);
            queue_blocked_write(&mut ctrl, &mut mem);
        }
        let backend = if failed_nsid == 1 {
            &mut ctrl.disk
        } else {
            ctrl.disk2.as_mut().unwrap()
        };
        set_raw_file_sync_failure(backend, Some(io::ErrorKind::Other));
        let status = notify(&mut ctrl, &mut mem, shn);
        assert_ne!(status & SHST_MASK, SHST_COMPLETE);
        assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
        assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);
        // This model treats an unreportable shutdown sync error as fatal.
        assert_eq!(status & (SHST_MASK | 2), (1 << 2) | 2);
        if shn == 2 {
            ctrl.mmio_write(REG_DOORBELL_BASE + 12, 4, 7);
        }
        let cq_before = mem
            .read_bytes(IO_CQ_BASE, usize::from(QDEPTH) * 16)
            .unwrap();
        assert_eq!(notify(&mut ctrl, &mut mem, shn), status);
        assert_eq!(raw_file_sync_attempts(&ctrl.disk), 1);
        assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 1);
        assert_eq!(
            ctrl.disk.read_at((7 * LBA_SIZE) as u64, LBA_SIZE).unwrap(),
            [0; LBA_SIZE]
        );
        assert_eq!(
            mem.read_bytes(IO_CQ_BASE, usize::from(QDEPTH) * 16)
                .unwrap(),
            cq_before
        );
        set_raw_file_sync_failure(&mut ctrl.disk, None);
        set_raw_file_sync_failure(ctrl.disk2.as_mut().unwrap(), None);
        ctrl.mmio_write(REG_CC, 4, CC_READY & !1);
        assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 0);
        ctrl.mmio_write(REG_CC, 4, CC_READY);
        assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 1);
        identify(&mut ctrl, &mut mem, 0, 0x37);
    }
}

#[test]
fn no_notification_and_reserved_encoding_preserve_existing_operation() {
    let files = ScratchFiles::new();
    let (mut ctrl, mut mem) = files.controller();
    // Reserved SHN=3 remains the existing no-op, not a new protocol promise.
    for (slot, shn) in [0, 3].into_iter().enumerate() {
        assert_eq!(notify(&mut ctrl, &mut mem, shn), 1);
        identify(&mut ctrl, &mut mem, slot as u16, 0x40 + slot as u16);
    }
    assert_eq!(raw_file_sync_attempts(&ctrl.disk), 0);
    assert_eq!(raw_file_sync_attempts(ctrl.disk2.as_ref().unwrap()), 0);
    ctrl.mmio_write(REG_CC, 4, CC_READY & !1);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 0);
    ctrl.mmio_write(REG_CC, 4, CC_READY);
    assert_eq!(ctrl.mmio_read(REG_CSTS, 4), 1);
    identify(&mut ctrl, &mut mem, 0, 0x42);
}
