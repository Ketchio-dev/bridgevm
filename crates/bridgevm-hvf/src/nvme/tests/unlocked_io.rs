//! READ and WRITE without the platform lock (unlocked_io.rs). The doorbell
//! queues the transfer; the vCPU runs it after unlocking, or hands it to its
//! worker and waits at most `NVME_IO_WAIT` (io_worker.rs), and a drain under
//! the lock posts the completion. `Mutex<(NvmeController, FakeMem)>` stands
//! in for the platform lock where another vCPU acts during a held transfer.

use super::super::flush_mode;
use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const IO_SQ_DOORBELL: u64 = REG_DOORBELL_BASE + 2 * 4;
const IO_CQ_DOORBELL: u64 = REG_DOORBELL_BASE + 3 * 4;
const SQ2_BASE: u64 = 0x4000_8000;
const CQ2_BASE: u64 = 0x4000_9000;
const SQ2_DOORBELL: u64 = REG_DOORBELL_BASE + 4 * 4;
const DATA2_BASE: u64 = 0x4000_a000;
const FUA: u32 = 1 << 30;

/// `take_io` for the tests, which run the transfers only in their `FakeMem`,
/// its `RawView`, or a worker over that view, and keep the `FakeMem` alive
/// until every transfer has returned (their workers are dropped first).
pub(super) trait QueuedIo {
    fn queued_io(&mut self) -> Option<NvmeIo>;
}

impl QueuedIo for NvmeController {
    fn queued_io(&mut self) -> Option<NvmeIo> {
        // SAFETY: as the trait says; each test's FakeMem outlives its transfers.
        unsafe { self.take_io() }
    }
}

/// Byte `lba + 1` fills each block of the test image.
pub(super) fn block_byte(lba: u32) -> u8 {
    lba as u8 + 1
}

/// A write-back raw file, one I/O queue pair, host pointers, async flush and
/// unlocked I/O on: the live VMM's data path.
pub(super) fn unlocked_controller(name: &str) -> (NvmeController, FakeMem, PathBuf) {
    let path = temp_path(name);
    let image: Vec<u8> = (0..16)
        .flat_map(|lba| [block_byte(lba); LBA_SIZE])
        .collect();
    fs::write(&path, image).unwrap();
    let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&path, true, 0x10000);
    mem.enable_host_ptr();
    ctrl.set_async_flush(true);
    ctrl.set_unlocked_io(true);
    create_io_queue_pair(
        &mut ctrl,
        &mut mem,
        0,
        CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT,
    );
    (ctrl, mem, path)
}

fn read(cid: u16, lba: u32, buf: u64) -> [u8; 64] {
    encode_sqe(NVM_OP_READ, cid, NSID, buf, lba, 0, 0)
}

pub(super) fn write(cid: u16, lba: u32, buf: u64, cdw12: u32) -> [u8; 64] {
    encode_sqe(NVM_OP_WRITE, cid, NSID, buf, lba, 0, cdw12)
}

/// The guest queues `sqe` at I/O SQ slot `slot` and rings the doorbell; the
/// events are the completions the doorbell exit posted.
pub(super) fn ring(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    sqe: &[u8; 64],
) -> Vec<NvmeCompletionEvent> {
    assert!(mem.write_bytes(IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE, sqe));
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, u64::from((slot + 1) % QDEPTH));
    ctrl.process(mem)
}

/// The drain of the vCPU's second hold.
pub(super) fn drain(ctrl: &mut NvmeController, mem: &mut FakeMem) -> Vec<NvmeCompletionEvent> {
    let mut events = Vec::new();
    ctrl.complete_flushes(mem, &mut events, false);
    events
}

pub(super) fn cqe(mem: &FakeMem, cq: u64, slot: u16) -> (u16, u16) {
    let entry = read_completion(mem, cq, slot);
    (
        u16::from_le_bytes([entry[12], entry[13]]),
        completion_status(&entry),
    )
}

fn byte_at(mem: &FakeMem, gpa: u64) -> u8 {
    mem.bytes[mem.at(gpa)]
}

pub(super) fn file_block(path: &PathBuf, lba: u32) -> Vec<u8> {
    let start = lba as usize * LBA_SIZE;
    fs::read(path).unwrap()[start..start + LBA_SIZE].to_vec()
}

/// Run `io` on another thread, held at the gate, and `act` meanwhile; a
/// third thread opens the gate after 50 ms. Returns what `act` returned and
/// how long it took.
fn while_held<R>(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    act: impl FnOnce(&mut NvmeController, &mut FakeMem) -> R,
) -> (R, Duration) {
    let gate = Arc::clone(&ctrl.io_gate);
    gate.close();
    let io = ctrl.queued_io().expect("the doorbell queued a transfer");
    let view = mem.view();
    std::thread::scope(|s| {
        s.spawn(move || io.run(&view));
        assert!(gate.reached(1), "the transfer never started");
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            gate.open();
        });
        let start = Instant::now();
        let result = act(ctrl, mem);
        (result, start.elapsed())
    })
}

/// A vCPU rings a READ and runs it as its MMIO exit does; while the
/// transfer is held, another thread tries the lock. Returns whether it got
/// the lock, with the READ's CQ slot and buffer as it saw them.
/// The READ's (command ID, status) and first buffer byte, as another thread
/// saw them during the transfer.
type Seen = Option<((u16, u16), u8)>;

fn read_held_by_a_vcpu(unlocked: bool) -> (Seen, FakeMem) {
    let (mut ctrl, mem, path) = unlocked_controller(&format!("unlocked-io-lock-{unlocked}"));
    ctrl.set_unlocked_io(unlocked);
    let gate = Arc::clone(&ctrl.io_gate);
    gate.close();
    let view = mem.view();
    let platform = Mutex::new((ctrl, mem));
    let seen = std::thread::scope(|s| {
        let vcpu = s.spawn(|| {
            let io = {
                let mut held = platform.lock().unwrap();
                let (ctrl, mem) = &mut *held;
                ring(ctrl, mem, 0, &read(0x11, 3, DATA_BASE));
                ctrl.queued_io()
            };
            if let Some(io) = io {
                io.run(&view);
                let mut held = platform.lock().unwrap();
                let (ctrl, mem) = &mut *held;
                assert_eq!(drain(ctrl, mem).len(), 1);
            }
        });
        assert!(gate.reached(1), "the READ never reached its transfer");
        let seen = platform
            .try_lock()
            .ok()
            .map(|held| (cqe(&held.1, IO_CQ_BASE, 0), byte_at(&held.1, DATA_BASE)));
        gate.open();
        vcpu.join().unwrap();
        seen
    });
    let (_, mem) = platform.into_inner().unwrap();
    let _ = fs::remove_file(path);
    (seen, mem)
}

#[test]
fn the_platform_lock_is_free_while_a_read_transfers() {
    let (seen, mem) = read_held_by_a_vcpu(true);
    assert_eq!(
        seen,
        Some(((0, 0), 0)),
        "another vCPU gets the lock and sees no completion and no data yet"
    );
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x11, SC_SUCCESS));
    assert_eq!(byte_at(&mem, DATA_BASE), block_byte(3));
}

#[test]
fn with_unlocked_io_off_the_read_holds_the_lock_as_before() {
    let (seen, mem) = read_held_by_a_vcpu(false);
    assert_eq!(seen, None, "the transfer ran under the doorbell's hold");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x11, SC_SUCCESS));
}

#[test]
fn two_vcpus_transfer_at_once_without_waiting_on_each_other() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-two-vcpus");
    // vCPU B's own queue pair, as stornvme gives each CPU one.
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | 2;
    let cq_flags = CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT | (1 << 16);
    let create_cq = encode_sqe(ADMIN_OP_CREATE_IO_CQ, 3, 0, CQ2_BASE, cdw10, cq_flags, 0);
    let create_sq = encode_sqe(
        ADMIN_OP_CREATE_IO_SQ,
        4,
        0,
        SQ2_BASE,
        cdw10,
        (2 << 16) | 1,
        0,
    );
    for (slot, sqe) in [(2, create_cq), (3, create_sq)] {
        submit_admin(&mut ctrl, &mut mem, slot, &sqe);
        assert_eq!(cqe(&mem, ACQ_BASE, slot).1, SC_SUCCESS);
    }
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    let gate = Arc::clone(&ctrl.io_gate);
    gate.close();
    let view = mem.view();
    let platform = Mutex::new((ctrl, mem));
    // Each vCPU: ring under the lock, transfer without it, drain under it.
    let vcpu = |sqe: [u8; 64], sq: u64, doorbell: u64, wait: bool| {
        let io = {
            let mut held = if wait {
                platform.lock().unwrap()
            } else {
                platform
                    .try_lock()
                    .expect("waited on the other vCPU's hold")
            };
            let (ctrl, mem) = &mut *held;
            assert!(mem.write_bytes(sq, &sqe));
            ctrl.mmio_write(doorbell, 4, 1);
            assert!(ctrl.process(mem).is_empty());
            ctrl.queued_io().expect("a queued transfer")
        };
        io.run(&view);
        let mut held = platform.lock().unwrap();
        let (ctrl, mem) = &mut *held;
        drain(ctrl, mem).len()
    };
    let posted = std::thread::scope(|s| {
        let a = s.spawn(|| {
            vcpu(
                write(0xa1, 2, DATA_BASE, 0),
                IO_SQ_BASE,
                IO_SQ_DOORBELL,
                true,
            )
        });
        assert!(gate.reached(1));
        let b = s.spawn(|| vcpu(read(0xb1, 5, DATA2_BASE), SQ2_BASE, SQ2_DOORBELL, false));
        assert!(gate.reached(2), "the two transfers never ran at once");
        {
            let held = platform.try_lock().expect("a transfer held the lock");
            assert_eq!(cqe(&held.1, IO_CQ_BASE, 0), (0, 0));
            assert_eq!(cqe(&held.1, CQ2_BASE, 0), (0, 0));
        }
        gate.open();
        a.join().unwrap() + b.join().unwrap()
    });
    let (_, mem) = platform.into_inner().unwrap();
    assert_eq!(
        posted, 2,
        "one completion each, posted by whichever drain came first"
    );
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0xa1, SC_SUCCESS));
    assert_eq!(cqe(&mem, CQ2_BASE, 0), (0xb1, SC_SUCCESS));
    assert_eq!(file_block(&path, 2), [0x5a; LBA_SIZE]);
    assert_eq!(byte_at(&mem, DATA2_BASE), block_byte(5));
    let _ = fs::remove_file(path);
}

#[test]
fn a_write_reaches_the_file_in_its_transfer_and_completes_after_it() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-write");
    assert!(mem.write_bytes(DATA_BASE, &[0x5a; LBA_SIZE]));
    assert!(ring(&mut ctrl, &mut mem, 0, &write(0x61, 4, DATA_BASE, 0)).is_empty());
    let io = ctrl.queued_io().expect("a queued transfer");
    assert_eq!(file_block(&path, 4), [block_byte(4); LBA_SIZE]);
    io.run(&mem);
    assert_eq!(file_block(&path, 4), [0x5a; LBA_SIZE]);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0, 0), "posted only by a drain");
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x61, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

#[test]
fn a_fua_write_completes_once_after_its_data_and_its_sync() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-fua");
    let syncs = flush_mode::recorded_syncs(&path).len();
    assert!(mem.write_bytes(DATA_BASE, &[0xa5; LBA_SIZE]));
    ring(&mut ctrl, &mut mem, 0, &write(0x71, 1, DATA_BASE, FUA));
    ctrl.queued_io().expect("a queued transfer").run(&mem);
    assert_eq!(file_block(&path, 1), [0xa5; LBA_SIZE]);
    assert!(
        drain(&mut ctrl, &mut mem).is_empty(),
        "its sync comes first"
    );
    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    assert_eq!(events.len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x71, SC_SUCCESS));
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0, 0), "exactly one completion");
    assert_eq!(flush_mode::recorded_syncs(&path).len(), syncs + 1);
    let _ = fs::remove_file(path);
}

#[test]
fn transfers_in_flight_hold_their_completion_slots() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-slots");
    // Seven reads take all seven usable slots of the 8-entry CQ.
    for slot in 0..7 {
        assert!(mem.write_bytes(
            IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE,
            &read(0x80 + slot, 0, DATA_BASE)
        ));
    }
    ctrl.mmio_write(IO_SQ_DOORBELL, 4, 7);
    ctrl.process(&mut mem);
    // The eighth waits, unfetched, while the seven are in flight.
    ring(&mut ctrl, &mut mem, 7, &read(0x87, 1, DATA_BASE));
    let io = ctrl.queued_io().expect("seven queued transfers");
    io.run(&mem);
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 7);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 6), (0x86, SC_SUCCESS));
    assert_eq!(
        cqe(&mem, IO_CQ_BASE, 7),
        (0, 0),
        "the eighth was not fetched"
    );
    // The guest consumes the seven; the eighth runs next.
    ctrl.mmio_write(IO_CQ_DOORBELL, 4, 7);
    ctrl.process(&mut mem);
    ctrl.queued_io().expect("the eighth").run(&mem);
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 7), (0x87, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

#[test]
fn a_reset_waits_for_a_held_read_and_a_recreated_queue_gets_no_stale_completion() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-reset");
    ring(&mut ctrl, &mut mem, 0, &read(0x21, 3, DATA_BASE));
    let ((), waited) = while_held(&mut ctrl, &mut mem, |ctrl, _| {
        ctrl.mmio_write(REG_CC, 4, 0);
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert_eq!(
        byte_at(&mem, DATA_BASE),
        block_byte(3),
        "the READ landed first"
    );
    assert!(!ctrl.flushes_in_flight() && !ctrl.flushes_ready());

    // The guest enables the controller again and re-creates the queue pair
    // in the same memory; only the new command completes there.
    assert!(mem.write_bytes(IO_CQ_BASE, &[0; 0x1000]));
    let aqa = (u32::from(QDEPTH - 1) << 16) | u32::from(QDEPTH - 1);
    ctrl.mmio_write(REG_AQA, 4, u64::from(aqa));
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    create_io_queue_pair(
        &mut ctrl,
        &mut mem,
        0,
        CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT,
    );
    ring(&mut ctrl, &mut mem, 0, &read(0x22, 5, DATA_BASE + 0x1000));
    ctrl.queued_io().expect("a queued transfer").run(&mem);
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x22, SC_SUCCESS));
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0, 0), "no stale completion");
    let _ = fs::remove_file(path);
}

#[test]
fn a_queue_deletion_waits_and_posts_the_held_read_first() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-delete-sq");
    ring(&mut ctrl, &mut mem, 0, &read(0x31, 3, DATA_BASE));
    let (events, waited) = while_held(&mut ctrl, &mut mem, |ctrl, mem| {
        let delete_sq = encode_sqe(ADMIN_OP_DELETE_IO_SQ, 0x32, 0, 0, 1, 0, 0);
        assert!(mem.write_bytes(ASQ_BASE + 2 * SQ_ENTRY_SIZE, &delete_sq));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 3);
        ctrl.process(mem)
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    let cqids: Vec<u16> = events.iter().map(|event| event.cqid).collect();
    assert_eq!(cqids, [1, 0], "the READ completes before the deletion");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x31, SC_SUCCESS));
    assert_eq!(cqe(&mem, ACQ_BASE, 2), (0x32, SC_SUCCESS));
    assert_eq!(byte_at(&mem, DATA_BASE), block_byte(3));
    let _ = fs::remove_file(path);
}

#[test]
fn a_shutdown_waits_for_a_held_write_before_its_durable_sync() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-shutdown");
    assert!(mem.write_bytes(DATA_BASE, &[0x77; LBA_SIZE]));
    ring(&mut ctrl, &mut mem, 0, &write(0x41, 6, DATA_BASE, 0));
    let ((), waited) = while_held(&mut ctrl, &mut mem, |ctrl, _| {
        ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT | (1 << 14)));
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert_eq!(
        (ctrl.mmio_read(REG_CSTS, 4) >> 2) & 3,
        2,
        "shutdown complete"
    );
    assert_eq!(file_block(&path, 6), [0x77; LBA_SIZE]);
    // Its completion still posts.
    assert_eq!(ctrl.process(&mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x41, SC_SUCCESS));
    let _ = fs::remove_file(path);
}

#[test]
fn a_guest_reset_waits_for_a_held_read_before_guest_ram_is_reused() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-guest-reset");
    ring(&mut ctrl, &mut mem, 0, &read(0x51, 7, DATA_BASE));
    let ((), waited) = while_held(&mut ctrl, &mut mem, |ctrl, _| {
        ctrl.reset_registers_keep_disks();
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert_eq!(
        byte_at(&mem, DATA_BASE),
        block_byte(7),
        "the READ landed first"
    );
    assert!(!ctrl.flushes_in_flight() && !ctrl.flushes_ready());
    assert!(ctrl.unlocked_io, "the next boot keeps the setting");
    let _ = fs::remove_file(path);
}

#[test]
fn other_disks_and_paths_stay_inline() {
    let path = temp_path("unlocked-io-inline");
    fs::write(&path, vec![0u8; LBA_SIZE * 8]).unwrap();
    // (memory disk, write-back file, host pointers, unlocked I/O)
    for (memory, write_back, host_ptr, unlocked) in [
        (true, true, true, true),
        (false, false, true, true),
        (false, true, false, true),
        (false, true, true, false),
    ] {
        let (mut ctrl, mut mem) = if memory {
            enabled_controller_with_mem_len(0x10000)
        } else {
            enabled_controller_with_raw_file(&path, write_back, 0x10000)
        };
        if host_ptr {
            mem.enable_host_ptr();
        }
        ctrl.set_unlocked_io(unlocked);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);
        assert_eq!(
            submit_io(&mut ctrl, &mut mem, 0, &read(1, 0, DATA_BASE)),
            SC_SUCCESS
        );
        assert!(
            ctrl.queued_io().is_none(),
            "{memory} {write_back} {host_ptr} {unlocked}"
        );
    }
    // An out-of-range READ fails inline with its usual status.
    let (mut ctrl, mut mem, path2) = unlocked_controller("unlocked-io-range");
    assert_eq!(
        submit_io(&mut ctrl, &mut mem, 0, &read(2, 16, DATA_BASE)),
        SC_INVALID_FIELD
    );
    assert!(ctrl.queued_io().is_none());
    for file in [path, path2] {
        let _ = fs::remove_file(file);
    }
}

#[test]
fn a_transfer_dropped_unrun_fails_without_touching_guest_ram() {
    let (mut ctrl, mut mem, path) = unlocked_controller("unlocked-io-dropped");
    ring(&mut ctrl, &mut mem, 0, &read(0x91, 3, DATA_BASE));
    drop(ctrl.queued_io().expect("a queued transfer"));
    // The wait a reset or admin command makes returns at once.
    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x91, SC_INTERNAL_DEVICE_ERROR));
    assert_eq!(byte_at(&mem, DATA_BASE), 0, "guest RAM untouched");
    let _ = fs::remove_file(path);
}

/// A transfer thread over `mem`'s bytes, as the probe gives each vCPU one.
fn worker_for(mem: &FakeMem) -> NvmeIoWorker {
    let view = mem.view();
    NvmeIoWorker::new("nvme-io-test", move || view)
}

pub(super) fn wait_until(mut ready: impl FnMut() -> bool) {
    let give_up = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < give_up, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The vCPU hands the queued transfer to its worker, which holds it at the
/// gate, and must be back within the wait; `act` then runs while a thread
/// releases the transfer 150 ms after the hand-off began. Returns what `act`
/// returned and how long it took.
fn while_detached<R>(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    act: impl FnOnce(&mut NvmeController, &mut FakeMem) -> R,
) -> (R, Duration) {
    let gate = Arc::clone(&ctrl.io_gate);
    gate.close();
    let io = ctrl.queued_io().expect("the doorbell queued a transfer");
    let mut worker = worker_for(mem);
    std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            gate.open();
        });
        let start = Instant::now();
        let posted_now = worker.run(io, &*mem, NVME_IO_WAIT);
        let returned = start.elapsed();
        assert!(
            !posted_now && returned < Duration::from_millis(50),
            "the vCPU waited {returned:?} for the held transfer"
        );
        assert!(gate.reached(1), "the worker holds the transfer");
        let start = Instant::now();
        let result = act(ctrl, mem);
        (result, start.elapsed())
    })
}

#[test]
fn a_held_read_returns_the_vcpu_and_completes_only_after_release() {
    let (mut ctrl, mut mem, path) = unlocked_controller("io-worker-detach");
    ring(&mut ctrl, &mut mem, 0, &read(0x13, 3, DATA_BASE));
    let (held, _) = while_detached(&mut ctrl, &mut mem, |ctrl, mem| {
        (
            drain(ctrl, mem).len(),
            cqe(mem, IO_CQ_BASE, 0),
            ctrl.flush_reserved_slots(1),
            ctrl.flushes_in_flight(),
        )
    });
    assert_eq!(
        held,
        (0, (0, 0), 1, true),
        "while held: no completion, its CQ slot kept, still in flight"
    );
    // After the release the worker publishes; the drain it asks for posts it.
    wait_until(|| ctrl.flushes_ready());
    assert_eq!(drain(&mut ctrl, &mut mem).len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x13, SC_SUCCESS));
    assert_eq!(byte_at(&mem, DATA_BASE), block_byte(3));
    let _ = fs::remove_file(path);
}

#[test]
fn a_reset_waits_for_a_read_its_vcpu_left_to_the_worker() {
    let (mut ctrl, mut mem, path) = unlocked_controller("io-worker-reset");
    ring(&mut ctrl, &mut mem, 0, &read(0x23, 3, DATA_BASE));
    let ((), waited) = while_detached(&mut ctrl, &mut mem, |ctrl, _| {
        ctrl.mmio_write(REG_CC, 4, 0);
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    assert_eq!(
        byte_at(&mem, DATA_BASE),
        block_byte(3),
        "the READ landed first"
    );
    assert!(
        !ctrl.flushes_in_flight() && !ctrl.flushes_ready(),
        "the reset dropped its completion with the queues"
    );
    let _ = fs::remove_file(path);
}

#[test]
fn a_queue_deletion_waits_for_a_read_its_vcpu_left_to_the_worker() {
    let (mut ctrl, mut mem, path) = unlocked_controller("io-worker-delete-sq");
    ring(&mut ctrl, &mut mem, 0, &read(0x33, 3, DATA_BASE));
    let (events, waited) = while_detached(&mut ctrl, &mut mem, |ctrl, mem| {
        let delete_sq = encode_sqe(ADMIN_OP_DELETE_IO_SQ, 0x34, 0, 0, 1, 0, 0);
        assert!(mem.write_bytes(ASQ_BASE + 2 * SQ_ENTRY_SIZE, &delete_sq));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 3);
        ctrl.process(mem)
    });
    assert!(
        waited >= Duration::from_millis(40),
        "returned after {waited:?}"
    );
    let cqids: Vec<u16> = events.iter().map(|event| event.cqid).collect();
    assert_eq!(cqids, [1, 0], "the READ completes before the deletion");
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x33, SC_SUCCESS));
    assert_eq!(cqe(&mem, ACQ_BASE, 2), (0x34, SC_SUCCESS));
    assert_eq!(byte_at(&mem, DATA_BASE), block_byte(3));
    let _ = fs::remove_file(path);
}

#[test]
fn a_fua_write_its_vcpu_left_to_the_worker_syncs_before_its_completion() {
    let (mut ctrl, mut mem, path) = unlocked_controller("io-worker-fua");
    let syncs = flush_mode::recorded_syncs(&path).len();
    assert!(mem.write_bytes(DATA_BASE, &[0xa5; LBA_SIZE]));
    ring(&mut ctrl, &mut mem, 0, &write(0x73, 1, DATA_BASE, FUA));
    while_detached(&mut ctrl, &mut mem, |_, _| ());
    wait_until(|| ctrl.flushes_ready());
    assert_eq!(file_block(&path, 1), [0xa5; LBA_SIZE]);
    assert!(
        drain(&mut ctrl, &mut mem).is_empty(),
        "its sync comes first"
    );
    let mut events = Vec::new();
    ctrl.complete_flushes(&mut mem, &mut events, true);
    assert_eq!(events.len(), 1);
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x73, SC_SUCCESS));
    assert_eq!(cqe(&mem, IO_CQ_BASE, 1), (0, 0), "exactly one completion");
    assert_eq!(flush_mode::recorded_syncs(&path).len(), syncs + 1);
    let _ = fs::remove_file(path);
}

#[test]
fn a_quick_transfer_runs_on_the_worker_and_its_vcpu_posts_it() {
    let (mut ctrl, mut mem, path) = unlocked_controller("io-worker-fast");
    ring(&mut ctrl, &mut mem, 0, &read(0x15, 2, DATA_BASE));
    let mut worker = worker_for(&mem);
    let io = ctrl.queued_io().expect("a queued transfer");
    // A wait no test host misses: this pins the path, not the timing.
    assert!(
        worker.run(io, &mem, Duration::from_secs(5)),
        "it finished within the wait"
    );
    assert_eq!(
        ctrl.io_gate.threads(),
        ["nvme-io-test"],
        "the worker ran it"
    );
    assert_eq!(
        drain(&mut ctrl, &mut mem).len(),
        1,
        "the vCPU's second hold posts it"
    );
    assert_eq!(cqe(&mem, IO_CQ_BASE, 0), (0x15, SC_SUCCESS));
    assert_eq!(byte_at(&mem, DATA_BASE), block_byte(2));
    let _ = fs::remove_file(path);
}
