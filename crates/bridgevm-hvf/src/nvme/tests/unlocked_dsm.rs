//! Deallocate without the platform lock (unlocked_io.rs, punch_order.rs).
//! The doorbell checks the range list and queues the punch, a vCPU's worker
//! punches, and a drain posts the completion, once. A WRITE fetched while the
//! punch is in flight, and overlapping it, runs after it; nothing else waits.

use super::super::dataset_management::test_punch;
use super::super::disk_full::faults::{self, Op};
use super::super::unlocked_io::test_gate::IoGate;
use super::super::*;
use super::helpers::*;
use super::unlocked_io::{
    block_byte, cqe, drain, file_block, ring, unlocked_controller, wait_until, write, QueuedIo,
};
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::{Duration, Instant};

const AD: u32 = 1 << 2;
/// Where the guest puts the range list.
const LIST: u64 = DATA_BASE + 0x1000;

/// A Deallocate of `blocks` LBAs from `slba`, its range list at `LIST`.
fn deallocate(mem: &mut FakeMem, cid: u16, slba: u64, blocks: u32) -> [u8; 64] {
    let mut range = [0u8; 16];
    range[4..8].copy_from_slice(&blocks.to_le_bytes());
    range[8..16].copy_from_slice(&slba.to_le_bytes());
    assert!(mem.write_bytes(LIST, &range));
    encode_sqe(NVM_OP_DATASET_MANAGEMENT, cid, NSID, LIST, 0, AD, 0)
}

/// LBAs 8-15: the second 4 KiB host block of the 8 KiB test image.
fn deallocate_second_block(mem: &mut FakeMem, cid: u16) -> [u8; 64] {
    deallocate(mem, cid, 8, 8)
}

fn punched(path: &PathBuf, except: Option<u32>) -> bool {
    (8..16)
        .filter(|&lba| Some(lba) != except)
        .all(|lba| file_block(path, lba) == [0; LBA_SIZE])
}

fn closed_gate() -> Arc<IoGate> {
    let gate = Arc::new(IoGate::default());
    gate.close();
    gate
}

/// Opens the gate when dropped, also while a failed assertion unwinds, so a
/// held job never keeps the test waiting for its thread.
struct OpenOnDrop<'a>(&'a IoGate);

impl Drop for OpenOnDrop<'_> {
    fn drop(&mut self) {
        self.0.open();
    }
}

/// Run the queued punch on another thread, held at a gate, and `act`
/// meanwhile; a third thread opens the gate after 50 ms. Returns what `act`
/// returned and how long it took.
fn while_punch_held<R>(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    act: impl FnOnce(&mut NvmeController, &mut FakeMem) -> R,
) -> (R, Duration) {
    let io = ctrl.queued_io().expect("a queued punch");
    let gate = closed_gate();
    let view = mem.view();
    std::thread::scope(|s| {
        let _open = OpenOnDrop(&gate);
        let held = Arc::clone(&gate);
        s.spawn(move || {
            test_punch::hold_at(held);
            io.run(&view);
        });
        assert!(gate.reached(1), "the punch is held");
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            gate.open();
        });
        let start = Instant::now();
        let result = act(ctrl, mem);
        (result, start.elapsed())
    })
}

#[test]
fn a_deallocate_punches_on_the_worker_and_completes_once_after_it() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-once");
    let sqe = deallocate_second_block(&mut mem, 0x81);
    assert!(
        ring(&mut ctrl, &mut mem, 0, &sqe).is_empty(),
        "the doorbell posts nothing"
    );
    assert_eq!(ctrl.flush_reserved_slots(1), 1, "its CQ slot is held");
    assert_eq!(ctrl.punches.len(), 1, "listed in flight");
    assert!(!punched(&path, None), "nothing is punched under the lock");
    let io = ctrl.queued_io().expect("the doorbell queued the punch");
    // The vCPU's worker holds the punch; the vCPU goes back to the guest.
    let gate = closed_gate();
    let held = Arc::clone(&gate);
    let view = mem.view();
    let mut worker = NvmeIoWorker::new("nvme-io-test", move || {
        test_punch::hold_at(held);
        view
    });
    let _open = OpenOnDrop(&gate);
    let start = Instant::now();
    assert!(!worker.run(io, &mem, NVME_IO_WAIT), "the vCPU did not wait");
    assert!(start.elapsed() < Duration::from_millis(50));
    assert!(gate.reached(1), "the worker holds the punch");
    assert!(drain(&mut ctrl, &mut mem).is_empty(), "nothing to post yet");
    assert!(!punched(&path, None));
    gate.open();
    wait_until(|| ctrl.flushes_ready());
    assert!(punched(&path, None));
    assert_eq!(gate.threads(), ["nvme-io-test"], "the worker punched");
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert!(drain(&mut ctrl, &mut mem).is_empty(), "posted once");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x81, SC_SUCCESS));
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0, 0), "exactly one completion");
    assert_eq!(ctrl.flush_reserved_slots(1), 0);
    assert!(ctrl.punches.is_empty(), "its drain forgot it");
    assert_eq!(file_block(&path, 7), [block_byte(7); LBA_SIZE]);
    drop(worker);
    let _ = fs::remove_file(path);
}

#[test]
fn a_write_after_a_held_punch_to_its_blocks_waits_and_lands_last_and_others_do_not() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-before-write");
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    let sqe = deallocate_second_block(&mut mem, 0x71);
    ring(&mut ctrl, &mut mem, 0, &sqe);
    let punch_io = ctrl.queued_io().expect("a queued punch");
    let punch = Arc::clone(&ctrl.punches[0]);
    let gate = closed_gate();
    let view = mem.view();
    std::thread::scope(|s| {
        let _open = OpenOnDrop(&gate);
        let held = Arc::clone(&gate);
        s.spawn(move || {
            test_punch::hold_at(held);
            punch_io.run(&view);
        });
        assert!(gate.reached(1), "the punch is held");
        // A WRITE elsewhere runs at once...
        ring(&mut ctrl, &mut mem, 1, &write(0x72, 2, DATA_BASE, 0));
        let io = ctrl.queued_io().expect("a queued WRITE");
        assert_eq!(io.punch_waits(), [0]);
        io.run(&mem);
        assert_eq!(file_block(&path, 2), [0x5a; LBA_SIZE]);
        // ...and one to a block being punched waits for the punch.
        ring(&mut ctrl, &mut mem, 2, &write(0x73, 9, DATA_BASE, 0));
        let write_io = ctrl.queued_io().expect("a queued WRITE");
        assert_eq!(write_io.punch_waits(), [1]);
        s.spawn(move || write_io.run(&view));
        wait_until(|| punch.waiters.load(SeqCst) == 1);
        assert_eq!(file_block(&path, 9), [block_byte(9); LBA_SIZE]);
        gate.open();
    });
    assert_eq!(
        file_block(&path, 9),
        [0x5a; LBA_SIZE],
        "the WRITE came last"
    );
    assert!(punched(&path, Some(9)));
    // The doorbell that queued the last WRITE posted the first one.
    drain(&mut ctrl, &mut mem);
    let mut posted: Vec<_> = (0..4).map(|slot| cqe(&mem, IO_CQ_BASE, slot)).collect();
    posted.sort_unstable();
    let done = [
        (0, 0),
        (0x71, SC_SUCCESS),
        (0x72, SC_SUCCESS),
        (0x73, SC_SUCCESS),
    ];
    assert_eq!(posted, done, "one completion each");
    assert!(ctrl.punches.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn with_no_punch_in_flight_a_write_takes_the_fast_path_untagged() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-fast-path");
    ring(&mut ctrl, &mut mem, 0, &write(0x31, 9, DATA_BASE, 0));
    assert!(ctrl.punches.is_empty());
    let io = ctrl.queued_io().expect("a queued WRITE");
    assert_eq!(io.punch_waits(), [0], "no tag");
    io.run(&mem);
    // A punch that returned stays listed until a drain, which the next
    // doorbell runs before it fetches: the WRITE after it is untagged again.
    let sqe = deallocate_second_block(&mut mem, 0x32);
    ring(&mut ctrl, &mut mem, 1, &sqe);
    ctrl.queued_io().expect("a queued punch").run(&mem);
    assert_eq!(ctrl.punches.len(), 1);
    ring(&mut ctrl, &mut mem, 2, &write(0x33, 9, DATA_BASE, 0));
    assert!(ctrl.punches.is_empty(), "the doorbell's drain forgot it");
    let io = ctrl.queued_io().expect("a queued WRITE");
    assert_eq!(io.punch_waits(), [0], "no tag");
    io.run(&mem);
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 2), (0x33, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

#[test]
fn a_reset_waits_for_a_held_punch() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-reset");
    let sqe = deallocate_second_block(&mut mem, 0x91);
    ring(&mut ctrl, &mut mem, 0, &sqe);
    let ((), waited) = while_punch_held(&mut ctrl, &mut mem, |ctrl, _| {
        ctrl.mmio_write(REG_CC, 4, 0);
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert!(punched(&path, None), "the punch ran first");
    assert!(
        !ctrl.flushes_in_flight() && !ctrl.flushes_ready(),
        "the reset dropped its completion with the queues"
    );
    assert!(ctrl.punches.is_empty());
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0, 0));
    let _ = fs::remove_file(path);
}

/// The VM exit's durable flush and a clean guest shutdown's sync.
#[test]
fn the_exit_flush_and_a_shutdown_wait_for_a_held_punch() {
    for shutdown in [false, true] {
        let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-flush");
        let sqe = deallocate_second_block(&mut mem, 0x95);
        ring(&mut ctrl, &mut mem, 0, &sqe);
        let ((), waited) = while_punch_held(&mut ctrl, &mut mem, |ctrl, _| {
            if shutdown {
                ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT | (1 << 14)));
            } else {
                ctrl.flush_disk().unwrap();
            }
        });
        assert!(
            waited >= Duration::from_millis(40),
            "{shutdown}: returned after {waited:?}"
        );
        assert!(punched(&path, None), "{shutdown}: the punch ran first");
        // Its completion still posts.
        assert_eq!(ctrl.process(&mut mem).len(), 1, "{shutdown}");
        assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x95, SC_SUCCESS), "{shutdown}");
        let _ = fs::remove_file(path);
    }
}

#[test]
fn the_disk_full_resume_waits_for_a_held_punch() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-disk-full");
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    faults::set(&path, Op::Write, Some(libc::ENOSPC));
    ring(&mut ctrl, &mut mem, 0, &write(0x41, 2, DATA_BASE, 0));
    ctrl.queued_io().expect("a queued WRITE").run(&mem);
    assert!(drain(&mut ctrl, &mut mem).is_empty(), "held for space");
    assert!(ctrl.disk_full().is_some());
    faults::set(&path, Op::Write, None);
    let sqe = deallocate_second_block(&mut mem, 0x42);
    ring(&mut ctrl, &mut mem, 1, &sqe);
    let (resumed, waited) = while_punch_held(&mut ctrl, &mut mem, |ctrl, mem| {
        ctrl.retry_held(mem, &mut Vec::new())
    });
    assert!(resumed, "nothing stays held");
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert!(punched(&path, None));
    assert_eq!(file_block(&path, 2), [0x5a; LBA_SIZE]);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x42, SC_SUCCESS), "the punch");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0x41, SC_SUCCESS), "the retry");
    let _ = fs::remove_file(path);
}

/// A WRITE that cannot be deferred (here: unlocked I/O switched off) and
/// overlaps a punch in flight waits for it under the lock.
#[test]
fn an_inline_write_over_a_held_punch_waits_for_it() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-inline-write");
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    let sqe = deallocate_second_block(&mut mem, 0x51);
    ring(&mut ctrl, &mut mem, 0, &sqe);
    ctrl.set_unlocked_io(false);
    let (events, waited) = while_punch_held(&mut ctrl, &mut mem, |ctrl, mem| {
        ring(ctrl, mem, 1, &write(0x52, 9, DATA_BASE, 0))
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert_eq!(events.len(), 1, "the WRITE completed inline");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x52, SC_SUCCESS));
    assert_eq!(file_block(&path, 9), [0x5a; LBA_SIZE], "after the punch");
    assert!(punched(&path, Some(9)));
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0x51, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

/// A punch its doorbell queued but no vCPU took yet would never run while
/// an inline WRITE waited for it: that WRITE runs the untaken jobs first.
#[test]
fn an_inline_write_runs_an_untaken_punch_under_it_first() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-untaken");
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    let sqe = deallocate_second_block(&mut mem, 0x61);
    ring(&mut ctrl, &mut mem, 0, &sqe);
    ctrl.set_unlocked_io(false);
    assert_eq!(
        ring(&mut ctrl, &mut mem, 1, &write(0x62, 9, DATA_BASE, 0)).len(),
        1
    );
    assert!(ctrl.queued_io().is_none(), "the punch was run");
    assert_eq!(file_block(&path, 9), [0x5a; LBA_SIZE], "after the punch");
    assert!(punched(&path, Some(9)));
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0x61, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

#[test]
fn a_deallocate_stays_inline_with_unlocked_io_off_or_a_bad_range() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-dsm-inline");
    // A range past the end completes at the doorbell with its error.
    let sqe = deallocate(&mut mem, 0xa1, 15, 2);
    assert_eq!(ring(&mut ctrl, &mut mem, 0, &sqe).len(), 1);
    assert_eq!(
        cqe(&mem, IO_CQ_BASE, 0),
        (0xa1, SC_LBA_OUT_OF_RANGE | SC_DNR)
    );
    assert!(!punched(&path, None));
    // BRIDGEVM_NVME_UNLOCKED_IO=0 and checkpoints: the punch runs under the
    // doorbell's hold, as before.
    ctrl.set_unlocked_io(false);
    let sqe = deallocate_second_block(&mut mem, 0xa2);
    assert_eq!(ring(&mut ctrl, &mut mem, 1, &sqe).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0xa2, SC_SUCCESS));
    assert!(punched(&path, None));
    assert!(ctrl.queued_io().is_none());
    assert!(ctrl.punches.is_empty());
    let _ = fs::remove_file(path);
}
