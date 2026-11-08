//! Deallocate punches off the platform lock. Punching a 1 GiB file's ranges
//! inline held the lock for 24-37 ms in one DSM command, longer than a 20 ms
//! audio buffer, and every vCPU that needed the lock stalled behind it.
//!
//! With unlocked I/O on (live VMMs; tests and checkpointing keep it off), a
//! DSM on a write-back raw file checks its range list as before, records the
//! valid byte ranges as pending punches of that namespace and completes at
//! once. A background thread, started on the first such DSM, punches them in
//! pieces of at most [`PIECE_BYTES`]. One mutex guards the pending ranges
//! and the piece in progress, and covers only that bookkeeping: the thread
//! takes a piece out of the pending ranges and records it as in progress,
//! unlocks, punches, locks again to clear it and wakes any waiter. A punch
//! usually takes microseconds, but one took 33 ms (likely behind a full sync
//! of the same file), and nothing under the platform lock may wait that long
//! for an unrelated piece.
//!
//! Deallocation is advisory (DLFEAT 0), so a range punched late, or never,
//! leaves the guest's data as valid as a punched one. What must never happen
//! is a punch of bytes the guest wrote after the DSM. Every WRITE fetched
//! from a submission queue takes its byte range out of the pending punches
//! in `process_sq`, under the mutex, before any path (inline, unlocked or
//! FUA) runs it, so no later piece touches its bytes. Only if the piece in
//! progress overlaps them does it wait, still before it runs, until that
//! punch returns: that is the guest writing into space it deallocated
//! milliseconds earlier, which is rare. Any other WRITE, and every DSM, only
//! waits for the bookkeeping. Each piece is rounded inward to whole host
//! blocks, so a block that a WRITE covers only in part keeps all its bytes.
//! When nothing is pending the check costs one atomic load. A WRITE fetched
//! before the DSM and still in flight may land before or after a punch of
//! the same bytes: NVMe orders nothing in flight, so both results are valid.
//! The final report's `nvme dsm:` record has the longest piece.
//!
//! Settling: a reset, a CC.EN 1 to 0 write or a snapshot restore drops the
//! pending punches and waits for the piece in progress; a wait for the jobs in
//! flight (shutdown through CC.SHN, the VM's exit sync, an export, a
//! namespace change) first punches them all, so a clean shutdown still gives
//! the space back before its durable sync. A namespace change also lets the
//! thread and its files go, and each namespace's ranges are kept with the
//! file they came from, so none reaches a different file. Dropping the
//! controller stops and joins the thread.

use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use super::dataset_management::{punch_ranges, DSM_STATS};
use super::flush_worker::lock;
#[cfg(test)]
use super::unlocked_io::test_gate::IoGate;
use super::*;

/// The most one piece punches. A piece ends at a multiple of it, which is a
/// multiple of any host block size, so splitting a range never leaves a
/// whole block unpunched.
pub(super) const PIECE_BYTES: u64 = 8 << 20;

/// Pending ranges per namespace (about 1.5 MiB at most); past it new ranges
/// are dropped, which only leaves their blocks allocated.
pub(super) const MAX_RANGES: usize = 65_536;

/// Disjoint byte ranges, start to end, none touching another.
#[derive(Default)]
pub(super) struct PendingRanges(BTreeMap<u64, u64>);

impl PendingRanges {
    /// Add `start..end`, merging the ranges it overlaps or touches; `false`
    /// when it is dropped at the bound.
    pub(super) fn add(&mut self, mut start: u64, mut end: u64) -> bool {
        if start >= end {
            return true;
        }
        if let Some((&s, &e)) = self.0.range(..=start).next_back() {
            if e >= start {
                self.0.remove(&s);
                (start, end) = (s, end.max(e));
            }
        }
        while let Some((&s, &e)) = self.0.range(start..=end).next() {
            self.0.remove(&s);
            end = end.max(e);
        }
        if self.0.len() >= MAX_RANGES {
            return false;
        }
        self.0.insert(start, end);
        true
    }

    /// Take `start..end` out; returns the ranges dropped because a split
    /// would pass the bound.
    pub(super) fn remove(&mut self, start: u64, end: u64) -> u64 {
        let mut dropped = 0;
        if let Some((&s, &e)) = self.0.range(..start).next_back() {
            if e > start {
                self.0.insert(s, start);
                if e > end {
                    if self.0.len() < MAX_RANGES {
                        self.0.insert(end, e);
                    } else {
                        dropped += 1;
                    }
                }
            }
        }
        while let Some((&s, &e)) = self.0.range(start..end).next() {
            self.0.remove(&s);
            if e > end {
                self.0.insert(end, e);
            }
        }
        dropped
    }

    /// The next piece: the lowest range up to the next multiple of
    /// [`PIECE_BYTES`].
    pub(super) fn take_piece(&mut self) -> Option<(u64, u64)> {
        let (start, end) = self.0.pop_first()?;
        let cut = (start / PIECE_BYTES + 1).saturating_mul(PIECE_BYTES);
        if end <= cut {
            return Some((start, end));
        }
        self.0.insert(cut, end);
        Some((start, cut))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(super) fn ranges(&self) -> Vec<(u64, u64)> {
        self.0.iter().map(|(&s, &e)| (s, e)).collect()
    }
}

/// One namespace's pending punches and the file they belong to.
struct Namespace {
    file: Arc<File>,
    ranges: PendingRanges,
}

/// The piece the thread is punching, with the mutex released.
struct Punching {
    slot: usize,
    start: u64,
    end: u64,
}

#[derive(Default)]
struct State {
    /// NSID 1 and 2.
    disks: [Option<Namespace>; 2],
    punching: Option<Punching>,
    stop: bool,
    /// Tests hold the thread before it takes a piece.
    #[cfg(test)]
    paused: bool,
}

impl State {
    fn any_pending(&self) -> bool {
        self.disks
            .iter()
            .flatten()
            .any(|disk| !disk.ranges.is_empty())
    }

    /// The thread has a piece to punch.
    fn ready(&self) -> bool {
        #[cfg(test)]
        if self.paused {
            return false;
        }
        self.any_pending()
    }

    /// The next piece, its namespace and its file.
    fn take_piece(&mut self) -> Option<(usize, Arc<File>, (u64, u64))> {
        let (slot, disk) = self.disks.iter_mut().enumerate().find_map(|(slot, disk)| {
            Some((slot, disk.as_mut()?)).filter(|(_, d)| !d.ranges.is_empty())
        })?;
        let piece = disk.ranges.take_piece()?;
        Some((slot, Arc::clone(&disk.file), piece))
    }

    /// The piece being punched overlaps `start..end` of namespace `slot`.
    fn punching_into(&self, slot: usize, start: u64, end: u64) -> bool {
        self.punching
            .as_ref()
            .is_some_and(|p| p.slot == slot && p.start < end && start < p.end)
    }
}

fn punch_piece(file: &File, piece: (u64, u64)) {
    let started = Instant::now();
    let punched = punch_ranges(file, &[piece]);
    DSM_STATS.note_piece(punched, started);
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// New ranges, the end of a piece, or stop.
    changed: Condvar,
    /// Some range is pending or being punched. Stored only under `state`,
    /// and cleared only after the last piece's punch has returned.
    pending: AtomicBool,
    /// Tests hold a piece after it is taken, before its punch.
    #[cfg(test)]
    gate: Arc<IoGate>,
}

impl Shared {
    fn publish(&self, state: &State) {
        let busy = state.any_pending() || state.punching.is_some();
        self.pending.store(busy, Ordering::Release);
    }

    /// Wait, with the mutex released, while the piece being punched
    /// satisfies `busy`.
    fn wait_while<'a>(
        &self,
        state: MutexGuard<'a, State>,
        mut busy: impl FnMut(&State) -> bool,
    ) -> MutexGuard<'a, State> {
        self.changed
            .wait_while(state, |state| busy(state))
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Take a piece under the mutex, punch it without, and clear it under
    /// the mutex again.
    fn run(&self) {
        let mut state = lock(&self.state);
        loop {
            state = self.wait_while(state, |state| !state.stop && !state.ready());
            if state.stop {
                return;
            }
            let Some((slot, file, piece)) = state.take_piece() else {
                continue;
            };
            let (start, end) = piece;
            state.punching = Some(Punching { slot, start, end });
            drop(state);
            #[cfg(test)]
            self.gate.pass();
            punch_piece(&file, piece);
            state = lock(&self.state);
            state.punching = None;
            self.publish(&state);
            self.changed.notify_all();
        }
    }
}

/// The controller's pending punches and the thread that runs them.
#[derive(Default)]
pub(crate) struct DeferredTrim {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for DeferredTrim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeferredTrim")
            .field("pending", &self.pending())
            .field("started", &self.thread.is_some())
            .finish()
    }
}

impl DeferredTrim {
    /// Whether any punch is pending or running: the one check every WRITE
    /// makes. A `false` read after the last piece means its punch returned.
    pub(crate) fn pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    /// Queue `ranges` of namespace `slot`'s `file` for the thread and return
    /// the ranges dropped at the bound, or `None` (punch inline) when the
    /// thread cannot start. Never waits for a piece.
    fn defer(&mut self, slot: usize, file: &Arc<File>, ranges: &[(u64, u64)]) -> Option<u64> {
        if self.thread.is_none() {
            match self.start() {
                Ok(thread) => self.thread = Some(thread),
                Err(error) => {
                    eprintln!("nvme: TRIM thread not started, punching inline: {error}");
                    return None;
                }
            }
        }
        let mut state = lock(&self.shared.state);
        let entry = &mut state.disks[slot];
        if entry
            .as_ref()
            .is_some_and(|disk| !Arc::ptr_eq(&disk.file, file))
        {
            *entry = None;
        }
        let disk = entry.get_or_insert_with(|| Namespace {
            file: Arc::clone(file),
            ranges: PendingRanges::default(),
        });
        let mut dropped = 0;
        for &(start, end) in ranges {
            dropped += u64::from(!disk.ranges.add(start, end));
        }
        self.shared.publish(&state);
        drop(state);
        self.shared.changed.notify_all();
        Some(dropped)
    }

    fn start(&self) -> io::Result<JoinHandle<()>> {
        let shared = Arc::clone(&self.shared);
        std::thread::Builder::new()
            .name("nvme-trim".into())
            .spawn(move || shared.run())
    }

    /// Take `start..end` of namespace `slot` out of the pending punches; if
    /// the piece being punched overlaps it, wait until that punch returns.
    fn keep(&self, slot: usize, start: u64, end: u64) {
        let mut state = lock(&self.shared.state);
        if let Some(disk) = state.disks[slot].as_mut() {
            DSM_STATS.note_dropped(disk.ranges.remove(start, end));
        }
        let state = self
            .shared
            .wait_while(state, |state| state.punching_into(slot, start, end));
        self.shared.publish(&state);
    }

    /// Drop the pending punches and wait for the piece being punched, so
    /// nothing is punched after this returns.
    pub(crate) fn discard(&self) {
        let mut state = lock(&self.shared.state);
        for disk in state.disks.iter_mut().flatten() {
            disk.ranges = PendingRanges::default();
        }
        let state = self
            .shared
            .wait_while(state, |state| state.punching.is_some());
        self.shared.publish(&state);
    }

    /// Punch every pending range before returning: wait for the piece being
    /// punched, then punch the rest on this thread.
    pub(crate) fn drain(&self) {
        let state = lock(&self.shared.state);
        let mut state = self
            .shared
            .wait_while(state, |state| state.punching.is_some());
        while let Some((_, file, piece)) = state.take_piece() {
            punch_piece(&file, piece);
        }
        self.shared.publish(&state);
    }
}

impl Drop for DeferredTrim {
    /// Stop the thread after the piece it is punching, and join it.
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        lock(&self.shared.state).stop = true;
        self.shared.changed.notify_all();
        let _ = thread.join();
    }
}

#[cfg(test)]
impl DeferredTrim {
    /// Hold or release the thread before it takes a piece; a drain still
    /// runs them.
    pub(crate) fn pause(&self, paused: bool) {
        lock(&self.shared.state).paused = paused;
        self.shared.changed.notify_all();
    }

    pub(crate) fn started(&self) -> bool {
        self.thread.is_some()
    }

    /// Closed, it holds each piece after the thread took it (it is in
    /// progress) and before its punch.
    pub(crate) fn piece_gate(&self) -> Arc<IoGate> {
        Arc::clone(&self.shared.gate)
    }

    /// The piece being punched.
    pub(crate) fn punching(&self) -> Option<(usize, u64, u64)> {
        let state = lock(&self.shared.state);
        state.punching.as_ref().map(|p| (p.slot, p.start, p.end))
    }

    /// Namespace `slot`'s pending ranges.
    pub(crate) fn ranges(&self, slot: usize) -> Vec<(u64, u64)> {
        lock(&self.shared.state).disks[slot]
            .as_ref()
            .map_or_else(Vec::new, |disk| disk.ranges.ranges())
    }
}

/// The pending-punch slot of namespace `nsid`.
fn slot(nsid: u32) -> Option<usize> {
    match nsid {
        NSID => Some(0),
        NSID2 => Some(1),
        _ => None,
    }
}

impl NvmeController {
    /// With unlocked I/O on, queue a write-back raw file's deallocated
    /// `ranges` for the punch thread; returns the ranges dropped at the
    /// bound, or `None` to punch them inline.
    pub(super) fn defer_punches(&mut self, nsid: u32, ranges: &[(u64, u64)]) -> Option<u64> {
        if !self.unlocked_io {
            return None;
        }
        let file = match self.backend_for_nsid(nsid) {
            Some(DiskBackend::RawFile(disk)) if disk.write_back => Arc::clone(&disk.file),
            _ => return None,
        };
        self.trim.defer(slot(nsid)?, &file, ranges)
    }

    /// A WRITE just fetched: no pending punch may touch its bytes. Called
    /// only while punches are pending; an invalid range writes nothing.
    pub(crate) fn keep_written(&self, cmd: &SubmissionEntry) {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return;
        };
        if let (Some(slot), Some((start, len))) = (slot(cmd.nsid), transfer_range(cmd, byte_len)) {
            self.trim.keep(slot, start, start + len as u64);
        }
    }
}
