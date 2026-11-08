//! Deallocate punches deferred to the punch thread (deferred_trim.rs): the
//! command completes before anything is punched, a later WRITE's bytes are
//! never punched, and resets drop the punches while shutdowns run them. The
//! tests hold the thread before it takes a piece with `pause`, or hold a
//! piece in progress at its gate, and release it when ready.

use super::super::deferred_trim::{PendingRanges, MAX_RANGES, PIECE_BYTES};
use super::super::unlocked_io::test_gate::IoGate;
use super::super::*;
use super::dataset_management::{
    allocated, dsm, dsm_on, image, intact, range, raw_controller, read, record_field,
    write_pattern, zeros, BPM, MIB,
};
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A raw write-back file with unlocked I/O on, as live VMMs run it, and the
/// punch thread held. The FakeMem exposes no host pointers until a test
/// enables them, so READs and WRITEs run inline unless it does.
fn held_controller(path: &Path) -> (NvmeController, FakeMem) {
    let (mut ctrl, mem) = raw_controller(path, true);
    ctrl.set_unlocked_io(true);
    ctrl.trim.pause(true);
    (ctrl, mem)
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let give_up = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < give_up, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Release the thread and wait until it has punched everything.
fn run_punches(ctrl: &NvmeController) {
    ctrl.trim.pause(false);
    wait_until(|| !ctrl.trim.pending());
}

/// A WRITE of `blocks` LBAs at `slba` from the guest buffer at `buf`.
fn write_sqe(cid: u16, slba: u64, blocks: u32, buf: u64) -> [u8; 64] {
    let prp2 = if blocks as usize * LBA_SIZE > PAGE_SIZE {
        buf + PAGE_SIZE as u64
    } else {
        0
    };
    encode_sqe_with_prps(
        NVM_OP_WRITE,
        cid,
        NSID,
        buf,
        prp2,
        [slba as u32, (slba >> 32) as u32, blocks - 1],
    )
}

#[cfg(target_os = "macos")]
#[test]
fn a_deallocate_completes_at_once_and_the_thread_punches_it_in_pieces() {
    let path = image("trim-deferred", 64 * MIB);
    write_pattern(&path, 8 * MIB, 48 * MIB);
    let before = allocated(&path);
    let (mut ctrl, mut mem) = held_controller(&path);
    let [deferred, punched, pieces] = ["deferred", "punched_bytes", "pieces"].map(record_field);

    assert_eq!(
        dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 32 * BPM)]),
        SC_SUCCESS
    );
    assert!(ctrl.trim.started() && ctrl.trim.pending());
    assert_eq!(allocated(&path), before, "nothing punched by the command");
    assert!(intact(&path, 8 * MIB, 48 * MIB));
    assert!(record_field("deferred") > deferred);

    run_punches(&ctrl);
    let after = allocated(&path);
    assert!(
        before - after >= 32 * MIB,
        "allocated {before} bytes before and {after} after"
    );
    assert!(zeros(&path, 16 * MIB, 32 * MIB));
    assert!(intact(&path, 8 * MIB, 8 * MIB) && intact(&path, 48 * MIB, 8 * MIB));
    // Other tests add to the record too.
    assert!(record_field("punched_bytes") >= punched + 32 * MIB);
    assert!(record_field("pieces") >= pieces + 4, "8 MiB pieces at most");
    drop(ctrl);
    let _ = fs::remove_file(path);
}

#[cfg(target_os = "macos")]
#[test]
fn a_write_after_the_deallocate_keeps_its_bytes_when_the_punches_run() {
    let path = image("trim-write-after", 64 * MIB);
    write_pattern(&path, 16 * MIB, 16 * MIB);
    let before = allocated(&path);
    let (mut ctrl, mut mem) = held_controller(&path);
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 16 * BPM)]),
        SC_SUCCESS
    );
    let buf = DATA_BASE + 0x4000;

    // 8 KiB at 18 MiB through the unlocked path: the doorbell queues it and
    // the vCPU runs it after unlocking.
    mem.enable_host_ptr();
    assert!(mem.write_bytes(buf, &[0x5a; 8192]));
    let sqe = write_sqe(0x41, 18 * BPM, 16, buf);
    assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
    ctrl.process(&mut mem);
    // SAFETY: run at once in this test's FakeMem, which outlives it.
    let io = unsafe { ctrl.take_io() }.expect("an unlocked WRITE");
    io.run(&mem);
    ctrl.complete_flushes(&mut mem, &mut Vec::new(), false);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
        SC_SUCCESS
    );

    // 1 KiB at 20 MiB + 512 inline, inside one 4 KiB host block.
    mem.expose_host_ptr = false;
    assert!(mem.write_bytes(buf, &[0xa5; 1024]));
    let sqe = write_sqe(0x42, 20 * BPM + 1, 2, buf);
    assert_eq!(submit_io(&mut ctrl, &mut mem, 2, &sqe), SC_SUCCESS);
    assert_eq!(
        ctrl.trim.ranges(0),
        [
            (16 * MIB, 18 * MIB),
            (18 * MIB + 8192, 20 * MIB + 512),
            (20 * MIB + 1536, 32 * MIB)
        ]
    );

    run_punches(&ctrl);
    assert!(
        read(&path, 18 * MIB, 8192) == [0x5a; 8192],
        "the unlocked WRITE's bytes were punched"
    );
    assert!(
        read(&path, 20 * MIB + 512, 1024) == [0xa5; 1024],
        "the inline WRITE's bytes were punched"
    );
    // The partly written block keeps its other bytes; the rest is a hole.
    assert!(intact(&path, 20 * MIB, 512) && intact(&path, 20 * MIB + 1536, 2560));
    assert!(zeros(&path, 16 * MIB, 2 * MIB));
    assert!(zeros(&path, 18 * MIB + 8192, 2 * MIB - 8192));
    assert!(zeros(&path, 20 * MIB + 4096, 12 * MIB - 4096));
    assert!(before - allocated(&path) >= 15 * MIB);
    drop(ctrl);
    let _ = fs::remove_file(path);
}

/// Holds the thread's next piece in progress: taken, its punch not begun.
/// The gate opens when this drops, so a failed assertion never leaves the
/// controller's drop waiting for the thread.
struct HeldPiece(Arc<IoGate>);

impl HeldPiece {
    /// Close the gate, release the paused thread and wait until it has taken
    /// its first piece.
    fn take(ctrl: &NvmeController) -> Self {
        let gate = ctrl.trim.piece_gate();
        gate.close();
        ctrl.trim.pause(false);
        assert!(gate.reached(1), "the thread took no piece");
        Self(gate)
    }

    /// Let the held punch run `after` from now, from another thread.
    fn release_after(&self, after: Duration) {
        let gate = Arc::clone(&self.0);
        std::thread::spawn(move || {
            std::thread::sleep(after);
            gate.open();
        });
    }
}

impl Drop for HeldPiece {
    fn drop(&mut self) {
        self.0.open();
    }
}

/// A punch can stall for tens of ms (one took 33 ms live), and the WRITEs
/// and DSMs that take the mutex run under the platform lock: only a WRITE
/// into the piece in progress may wait for it.
#[cfg(target_os = "macos")]
#[test]
fn a_slow_piece_holds_up_only_a_write_into_its_own_bytes() {
    let path = image("trim-held-piece", 64 * MIB);
    write_pattern(&path, 16 * MIB, 32 * MIB);
    let (mut ctrl, mut mem) = held_controller(&path);
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 16 * BPM)]),
        SC_SUCCESS
    );
    let held = HeldPiece::take(&ctrl);
    assert_eq!(ctrl.trim.punching(), Some((0, 16 * MIB, 24 * MIB)));
    let started = Instant::now();
    held.release_after(Duration::from_secs(1));

    // A DSM, and a WRITE into the pending piece after the held one: neither
    // waits for the held punch.
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 1, &[range(40 * BPM, 8 * BPM)]),
        SC_SUCCESS
    );
    let buf = DATA_BASE + 0x4000;
    assert!(mem.write_bytes(buf, &[0x5a; 4096]));
    let sqe = write_sqe(0x61, 28 * BPM, 8, buf);
    assert_eq!(submit_io(&mut ctrl, &mut mem, 2, &sqe), SC_SUCCESS);
    let unrelated = started.elapsed();
    let still_held = ctrl.trim.punching().is_some() && intact(&path, 16 * MIB, 8 * MIB);

    // A WRITE into the held piece waits until its punch has returned, and
    // only then lands.
    assert!(mem.write_bytes(buf, &[0xa5; 4096]));
    let sqe = write_sqe(0x62, 18 * BPM, 8, buf);
    assert_eq!(submit_io(&mut ctrl, &mut mem, 3, &sqe), SC_SUCCESS);
    let overlapping = started.elapsed();

    run_punches(&ctrl);
    assert!(
        read(&path, 18 * MIB, 4096) == [0xa5; 4096],
        "the WRITE into the held piece landed before its punch"
    );
    assert!(read(&path, 28 * MIB, 4096) == [0x5a; 4096]);
    assert!(zeros(&path, 16 * MIB, 2 * MIB) && zeros(&path, 18 * MIB + 4096, 10 * MIB - 4096));
    assert!(zeros(&path, 28 * MIB + 4096, 4 * MIB - 4096) && zeros(&path, 40 * MIB, 8 * MIB));
    assert!(intact(&path, 32 * MIB, 8 * MIB));
    assert!(
        unrelated < Duration::from_millis(500) && still_held,
        "the DSM and the unrelated WRITE waited {unrelated:?} for the held piece"
    );
    assert!(
        overlapping >= Duration::from_millis(900),
        "the overlapping WRITE returned after {overlapping:?}, before the punch"
    );
    drop(ctrl);
    let _ = fs::remove_file(path);
}

/// A reset and a shutdown wait for the piece in progress: nothing is punched
/// after a reset returns, and a shutdown's sync comes after every punch.
#[cfg(target_os = "macos")]
#[test]
fn a_reset_or_a_shutdown_waits_for_the_piece_in_progress() {
    for exit in [false, true] {
        let path = image(&format!("trim-held-settle-{exit}"), 64 * MIB);
        write_pattern(&path, 16 * MIB, 16 * MIB);
        let (mut ctrl, mut mem) = held_controller(&path);
        assert_eq!(
            dsm(&mut ctrl, &mut mem, 0, &[range(16 * BPM, 16 * BPM)]),
            SC_SUCCESS
        );
        let held = HeldPiece::take(&ctrl);
        let started = Instant::now();
        held.release_after(Duration::from_millis(300));
        if exit {
            ctrl.flush_disk().unwrap();
        } else {
            ctrl.mmio_write(REG_CC, 4, 0);
        }
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(250),
            "returned after {waited:?}"
        );
        assert!(!ctrl.trim.pending() && ctrl.trim.punching().is_none());
        assert!(zeros(&path, 16 * MIB, 8 * MIB), "the held piece ran first");
        if exit {
            assert!(
                zeros(&path, 24 * MIB, 8 * MIB),
                "the drain punched the rest"
            );
        } else {
            run_punches(&ctrl);
            assert!(
                intact(&path, 24 * MIB, 8 * MIB),
                "the reset dropped the rest"
            );
        }
        drop(ctrl);
        let _ = fs::remove_file(path);
    }
}

#[test]
fn a_reset_or_a_restore_drops_the_pending_punches() {
    for settle in ["disable", "restore", "reset"] {
        let path = image(&format!("trim-drop-{settle}"), 32 * MIB);
        write_pattern(&path, 8 * MIB, 16 * MIB);
        let before = allocated(&path);
        let (mut ctrl, mut mem) = held_controller(&path);
        let snapshot = ctrl.snapshot_state();
        assert_eq!(
            dsm(&mut ctrl, &mut mem, 0, &[range(8 * BPM, 16 * BPM)]),
            SC_SUCCESS
        );
        assert!(ctrl.trim.pending(), "{settle}");
        match settle {
            "disable" => ctrl.mmio_write(REG_CC, 4, 0),
            "restore" => ctrl.restore_state(&snapshot),
            _ => ctrl.reset_registers_keep_disks(),
        }
        assert!(!ctrl.trim.pending(), "{settle}");
        // Were any left, the released thread would punch them now.
        run_punches(&ctrl);
        assert!(intact(&path, 8 * MIB, 16 * MIB), "{settle}");
        assert_eq!(allocated(&path), before, "{settle}");
        drop(ctrl);
        let _ = fs::remove_file(path);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_shutdown_or_the_exit_sync_punches_the_pending_ranges_first() {
    for exit in [false, true] {
        let path = image(&format!("trim-drain-{exit}"), 32 * MIB);
        write_pattern(&path, 8 * MIB, 16 * MIB);
        let before = allocated(&path);
        let (mut ctrl, mut mem) = held_controller(&path);
        assert_eq!(
            dsm(&mut ctrl, &mut mem, 0, &[range(8 * BPM, 16 * BPM)]),
            SC_SUCCESS
        );
        if exit {
            ctrl.flush_disk().unwrap();
        } else {
            ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT | (1 << 14)));
            assert_eq!((ctrl.mmio_read(REG_CSTS, 4) >> 2) & 3, 2, "shut down");
        }
        // Still held: the caller punched them.
        assert!(!ctrl.trim.pending());
        assert!(zeros(&path, 8 * MIB, 16 * MIB), "exit {exit}");
        assert!(before - allocated(&path) >= 16 * MIB, "exit {exit}");
        drop(ctrl);
        let _ = fs::remove_file(path);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_namespace_change_punches_the_old_file_and_never_the_new_one() {
    let primary = image("trim-ns-primary", 8 * MIB);
    let [old, new] = ["trim-ns-old", "trim-ns-new"].map(|name| {
        let path = image(name, 32 * MIB);
        write_pattern(&path, 8 * MIB, 16 * MIB);
        path
    });
    let (mut ctrl, mut mem) = raw_controller(&primary, true);
    ctrl.attach_second_namespace_raw_file(&old, true).unwrap();
    ctrl.set_unlocked_io(true);
    ctrl.trim.pause(true);
    assert_eq!(
        dsm_on(&mut ctrl, &mut mem, 0, NSID2, &[range(8 * BPM, 16 * BPM)]),
        SC_SUCCESS
    );
    assert!(intact(&old, 8 * MIB, 16 * MIB));
    ctrl.attach_second_namespace_raw_file(&new, true).unwrap();
    assert!(zeros(&old, 8 * MIB, 16 * MIB), "punched into the old file");
    assert!(!ctrl.trim.pending() && !ctrl.trim.started());
    assert!(intact(&new, 8 * MIB, 16 * MIB));
    // The next Deallocate starts a thread on the new file.
    assert_eq!(
        dsm_on(&mut ctrl, &mut mem, 1, NSID2, &[range(12 * BPM, 4 * BPM)]),
        SC_SUCCESS
    );
    run_punches(&ctrl);
    assert!(zeros(&new, 12 * MIB, 4 * MIB));
    assert!(intact(&new, 8 * MIB, 4 * MIB) && intact(&new, 16 * MIB, 8 * MIB));
    drop(ctrl);
    for path in [primary, old, new] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn with_unlocked_io_on_bad_lists_and_other_disks_queue_nothing() {
    // A range past the end fails as inline, before anything is recorded.
    let path = image("trim-bad-list", MIB);
    write_pattern(&path, 0, MIB);
    let (mut ctrl, mut mem) = held_controller(&path);
    assert_eq!(
        dsm(&mut ctrl, &mut mem, 0, &[range(BPM - 8, 9)]),
        SC_LBA_OUT_OF_RANGE | SC_DNR
    );
    assert!(!ctrl.trim.pending() && !ctrl.trim.started());
    let _ = fs::remove_file(path);

    // A read-only raw file's writes live in its overlay: nothing to punch.
    let path = image("trim-read-only", MIB);
    let (mut ctrl, mut mem) = raw_controller(&path, false);
    ctrl.set_unlocked_io(true);
    assert_eq!(dsm(&mut ctrl, &mut mem, 0, &[range(0, BPM)]), SC_SUCCESS);
    assert!(!ctrl.trim.pending() && !ctrl.trim.started());
    let _ = fs::remove_file(path);
}

#[test]
fn pending_ranges_merge_split_cut_into_pieces_and_stop_at_the_bound() {
    let mut set = PendingRanges::default();
    for (start, end) in [(100, 200), (300, 400), (200, 250), (390, 500), (150, 160)] {
        assert!(set.add(start, end));
    }
    assert_eq!(set.ranges(), [(100, 250), (300, 500)]);
    assert!(set.add(250, 300), "touching both");
    assert_eq!(set.ranges(), [(100, 500)]);
    for (start, end) in [(120, 130), (90, 110), (480, 600)] {
        assert_eq!(set.remove(start, end), 0);
    }
    assert_eq!(set.ranges(), [(110, 120), (130, 480)]);

    // A piece ends at the next multiple of PIECE_BYTES.
    let p = PIECE_BYTES;
    let mut set = PendingRanges::default();
    assert!(set.add(p - 4096, 3 * p + 512));
    let pieces: Vec<_> = std::iter::from_fn(|| set.take_piece()).collect();
    assert_eq!(
        pieces,
        [
            (p - 4096, p),
            (p, 2 * p),
            (2 * p, 3 * p),
            (3 * p, 3 * p + 512)
        ]
    );

    // Past the bound a new range is dropped, and so is a split's tail.
    let mut set = PendingRanges::default();
    for i in 0..MAX_RANGES as u64 {
        assert!(set.add(i * 10, i * 10 + 5));
    }
    assert!(!set.add(1 << 40, (1 << 40) + 5));
    assert!(set.add(5, 10), "a merge needs no room");
    assert_eq!(set.remove(1001, 1002), 0, "room for one split");
    assert_eq!(set.remove(2001, 2002), 1, "its tail is dropped");
    assert!(set.ranges().contains(&(2000, 2001)));
}
