//! A full host disk holds the WRITE or FLUSH instead of completing it, and
//! RESUME's retry completes it (disk_full.rs). Faults are injected per file
//! before the host call, so the file shows what reached it.

use super::super::disk_full::faults::{self, Op};
use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;

const IO_SQ_DOORBELL: u64 = REG_DOORBELL_BASE + 2 * 4;
const FUA: u32 = 1 << 30;

/// Byte `lba + 1` fills each block of the test image.
fn image_block(lba: u32) -> Vec<u8> {
    vec![lba as u8 + 1; LBA_SIZE]
}

/// A write-back raw file and one I/O queue pair; `live` adds host pointers,
/// async flush and unlocked I/O, the live VMM's data path.
fn controller(name: &str, live: bool) -> (NvmeController, FakeMem, PathBuf) {
    let path = temp_path(name);
    fs::write(&path, (0..16).flat_map(image_block).collect::<Vec<u8>>()).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&path, true, 0x10000);
    if live {
        mem.enable_host_ptr();
        ctrl.set_async_flush(true);
        ctrl.set_unlocked_io(true);
    }
    create_io_queue_pair(
        &mut ctrl,
        &mut mem,
        0,
        CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT,
    );
    (ctrl, mem, path)
}

/// One block at `lba` from its own guest page.
fn write(cid: u16, lba: u32, cdw12: u32) -> [u8; 64] {
    let buf = DATA_BASE + u64::from(lba) * 0x1000;
    encode_sqe(NVM_OP_WRITE, cid, NSID, buf, lba, 0, cdw12)
}

fn flush(cid: u16) -> [u8; 64] {
    encode_sqe(NVM_OP_FLUSH, cid, NSID, 0, 0, 0, 0)
}

/// The guest fills the WRITE buffer for `lba` with `byte`.
fn fill(mem: &mut FakeMem, lba: u32, byte: u8) {
    let buf = DATA_BASE + u64::from(lba) * 0x1000;
    assert!(mem.write_bytes(buf, &[byte; LBA_SIZE]));
}

/// The guest queues `sqe` at I/O SQ slot `slot` and rings the doorbell; any
/// transfer it queued runs, and a drain waits for the jobs in flight.
fn ring(ctrl: &mut NvmeController, mem: &mut FakeMem, slot: u16, sqe: &[u8; 64]) {
    assert!(mem.write_bytes(IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE, sqe));
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, u64::from(slot + 1));
    ctrl.process(mem);
    // SAFETY: run here, in this FakeMem's view, while the FakeMem lives.
    if let Some(io) = unsafe { ctrl.take_io() } {
        io.run(&mem.view());
    }
    ctrl.complete_flushes(mem, &mut Vec::new(), true);
}

/// (command ID, status) at CQ slot `slot`, once posted (phase tag set).
fn cqe(mem: &FakeMem, slot: u16) -> Option<(u16, u16)> {
    let entry = read_completion(mem, IO_CQ_BASE, slot);
    (entry[14] & 1 == 1).then(|| {
        (
            u16::from_le_bytes([entry[12], entry[13]]),
            completion_status(&entry),
        )
    })
}

fn file_block(path: &PathBuf, lba: u32) -> Vec<u8> {
    let start = lba as usize * LBA_SIZE;
    fs::read(path).unwrap()[start..start + LBA_SIZE].to_vec()
}

fn held_errno(ctrl: &NvmeController) -> Option<i32> {
    ctrl.disk_full().map(|full| full.errno)
}

/// RESUME's retry: whether nothing stays held, and the interrupts it raised.
fn retry(ctrl: &mut NvmeController, mem: &mut FakeMem) -> (bool, usize) {
    let mut events = Vec::new();
    let done = ctrl.retry_held(mem, &mut events);
    (done, events.len())
}

/// Inline WRITE through the buffered and the direct path, and a WRITE run
/// without the lock: no completion, the flag up, the file untouched, the
/// completion slot kept. A retry on a disk still full holds it again; once
/// there is space the data lands and the guest gets SUCCESS.
#[test]
fn a_write_on_a_full_disk_completes_only_after_the_retry() {
    for (name, live, direct) in [
        ("disk-full-buffered", false, false),
        ("disk-full-direct", false, true),
        ("disk-full-unlocked", true, true),
    ] {
        let (mut ctrl, mut mem, path) = controller(name, live);
        if direct {
            mem.enable_host_ptr();
        }
        let flag = ctrl.storage_full_flag();
        faults::set(&path, Op::Write, Some(libc::ENOSPC));
        fill(&mut mem, 3, 0xab);
        ring(&mut ctrl, &mut mem, 0, &write(0x21, 3, 0));
        assert_eq!(cqe(&mem, 0), None, "{name}: no completion");
        assert!(flag.load(SeqCst), "{name}");
        assert_eq!(held_errno(&ctrl), Some(libc::ENOSPC), "{name}");
        assert_eq!(
            ctrl.flush_reserved_slots(1),
            1,
            "{name}: its CQ slot stays held"
        );
        assert_eq!(
            file_block(&path, 3),
            image_block(3),
            "{name}: nothing written"
        );

        assert_eq!(retry(&mut ctrl, &mut mem), (false, 0), "{name}: still full");
        assert_eq!(cqe(&mem, 0), None, "{name}");
        assert!(flag.load(SeqCst), "{name}");

        faults::set(&path, Op::Write, None);
        assert_eq!(
            retry(&mut ctrl, &mut mem),
            (true, 1),
            "{name}: with its interrupt"
        );
        assert_eq!(cqe(&mem, 0), Some((0x21, SC_SUCCESS)), "{name}");
        assert_eq!(file_block(&path, 3), vec![0xab; LBA_SIZE], "{name}");
        assert!(!flag.load(SeqCst), "{name}");
        assert_eq!(ctrl.disk_full(), None, "{name}");
        assert_eq!(ctrl.flush_reserved_slots(1), 0, "{name}");
        let _ = fs::remove_file(path);
    }
}

/// A FLUSH whose sync finds the volume over quota, inline and on the flush
/// worker, and a FUA WRITE run without the lock whose data landed but whose
/// sync did not: no completion until the retry's sync succeeds.
#[test]
fn a_sync_on_a_full_disk_completes_only_after_the_retry() {
    for (name, live, sqe) in [
        ("disk-full-flush-inline", false, flush(0x31)),
        ("disk-full-flush-worker", true, flush(0x31)),
        ("disk-full-fua", true, write(0x31, 0, FUA)),
    ] {
        let (mut ctrl, mut mem, path) = controller(name, live);
        faults::set(&path, Op::Sync, Some(libc::EDQUOT));
        fill(&mut mem, 0, 0xcd);
        ring(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(cqe(&mem, 0), None, "{name}: not acknowledged");
        assert_eq!(held_errno(&ctrl), Some(libc::EDQUOT), "{name}");
        assert_eq!(retry(&mut ctrl, &mut mem), (false, 0), "{name}: still full");
        assert_eq!(cqe(&mem, 0), None, "{name}");

        faults::set(&path, Op::Sync, None);
        assert_eq!(retry(&mut ctrl, &mut mem), (true, 1), "{name}");
        assert_eq!(cqe(&mem, 0), Some((0x31, SC_SUCCESS)), "{name}");
        assert_eq!(ctrl.disk_full(), None, "{name}");
        let _ = fs::remove_file(path);
    }
}

/// Held commands retry in the order they were held; a reset drops them and
/// lowers the flag vCPU0 polls.
#[test]
fn held_commands_retry_in_order_and_a_reset_drops_them() {
    let (mut ctrl, mut mem, path) = controller("disk-full-order", false);
    let flag = ctrl.storage_full_flag();
    faults::set(&path, Op::Write, Some(libc::ENOSPC));
    for (slot, lba) in [(0, 4), (1, 2)] {
        fill(&mut mem, lba, 0x60 + lba as u8);
        ring(&mut ctrl, &mut mem, slot, &write(0x50 + slot, lba, 0));
    }
    assert_eq!(ctrl.flush_reserved_slots(1), 2);
    faults::set(&path, Op::Write, None);
    assert_eq!(retry(&mut ctrl, &mut mem), (true, 2));
    assert_eq!(cqe(&mem, 0), Some((0x50, SC_SUCCESS)));
    assert_eq!(cqe(&mem, 1), Some((0x51, SC_SUCCESS)));
    assert_eq!(file_block(&path, 4), vec![0x64; LBA_SIZE]);
    assert_eq!(file_block(&path, 2), vec![0x62; LBA_SIZE]);

    faults::set(&path, Op::Write, Some(libc::ENOSPC));
    ring(&mut ctrl, &mut mem, 2, &write(0x52, 5, 0));
    assert!(flag.load(SeqCst));
    ctrl.reset_registers_keep_disks();
    assert!(
        !flag.load(SeqCst),
        "the flag vCPU0 holds is the one lowered"
    );
    assert_eq!(ctrl.disk_full(), None);
    assert!(Arc::ptr_eq(&flag, &ctrl.storage_full_flag()));
    let _ = fs::remove_file(path);
}

/// A transfer that fails once the pause has begun (published, not yet
/// drained) joins the same RESUME's retry, after the one held before it.
#[test]
fn a_write_that_fails_during_the_pause_joins_the_retry() {
    let (mut ctrl, mut mem, path) = controller("disk-full-late", true);
    faults::set(&path, Op::Write, Some(libc::ENOSPC));
    ring(&mut ctrl, &mut mem, 0, &write(0x81, 1, 0));
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &write(0x82, 2, 0)));
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 2);
    ctrl.process(&mut mem);
    // SAFETY: run here, in this FakeMem's view, while the FakeMem lives.
    unsafe { ctrl.take_io() }
        .expect("a queued WRITE")
        .run(&mem.view());

    faults::set(&path, Op::Write, None);
    assert_eq!(retry(&mut ctrl, &mut mem), (true, 2));
    assert_eq!(cqe(&mem, 0), Some((0x81, SC_SUCCESS)));
    assert_eq!(cqe(&mem, 1), Some((0x82, SC_SUCCESS)));
    let _ = fs::remove_file(path);
}

/// Other host errors complete as before: a WRITE with Invalid Field, a sync
/// with Internal Device Error; nothing is held.
#[test]
fn other_host_errors_complete_as_before() {
    for (name, live, op, sqe, status) in [
        (
            "disk-eio-write",
            false,
            Op::Write,
            write(0x71, 1, 0),
            SC_INVALID_FIELD,
        ),
        (
            "disk-eio-unlocked",
            true,
            Op::Write,
            write(0x71, 1, 0),
            SC_INVALID_FIELD,
        ),
        (
            "disk-eio-flush",
            true,
            Op::Sync,
            flush(0x71),
            SC_INTERNAL_DEVICE_ERROR,
        ),
    ] {
        let (mut ctrl, mut mem, path) = controller(name, live);
        faults::set(&path, op, Some(libc::EIO));
        ring(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(cqe(&mem, 0), Some((0x71, status)), "{name}");
        assert_eq!(ctrl.disk_full(), None, "{name}");
        assert!(!ctrl.storage_full_flag().load(SeqCst), "{name}");
        let _ = fs::remove_file(path);
    }
}
