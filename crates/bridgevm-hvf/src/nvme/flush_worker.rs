//! NVM FLUSH off the vCPU. A raw-file disk's `F_FULLFSYNC` takes about 1.5 ms
//! and tens of ms under load, and it ran inside the doorbell write, under the
//! platform lock, so every vCPU stalled behind each guest flush (guest timer
//! gaps up to 26 ms against 4-7 ms on QEMU/HVF while another thread flushed).
//!
//! With async flush on, an I/O-queue FLUSH for a write-back raw file goes to a
//! worker thread that syncs duplicated handles of the disk files, one sync per
//! disk for every flush queued meanwhile, and keeps each disk's ordered-mode
//! debt (flush_mode.rs). The command completes, with the same status rules,
//! only after that sync returns: the pre-run drain posts the completion, and
//! `FLUSH_DONE_HOOK` asks a vCPU to run that drain at once. FLUSH covers the
//! writes completed before it was submitted (NVMe 1.4 6.8); those are in the
//! host page cache by then, so the later sync covers them.
//!
//! A controller register write, any admin command and a reset first wait for
//! the flushes in flight, so queue deletion and reset never meet a pending
//! completion; a reset drops their completions with the queues they targeted.
//! READs and WRITEs run without the platform lock (unlocked_io.rs) are counted
//! and completed here too.

use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Instant;

use super::flush_mode::DiskSync;
use super::unlocked_io::IoJob;
use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Called by the worker after each batch of syncs; the live VMM kicks a vCPU
/// so its pre-run drain posts the completions.
pub static FLUSH_DONE_HOOK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

#[derive(Clone, Copy)]
pub(super) struct FlushJob {
    pub(super) sqid: u16,
    pub(super) cqid: u16,
    pub(super) cmd: SubmissionEntry,
    disk: bool,
    disk2: bool,
}

impl FlushJob {
    /// A job with nothing for the worker to sync: a READ or WRITE.
    pub(super) fn io(sqid: u16, cqid: u16, cmd: SubmissionEntry) -> Self {
        Self {
            sqid,
            cqid,
            cmd,
            disk: false,
            disk2: false,
        }
    }
}

pub(super) struct Finished {
    pub(super) job: FlushJob,
    pub(super) status: u16,
    /// A FUA or write-cache-off WRITE whose data is written: its sync comes
    /// next, then its completion.
    pub(super) sync_next: bool,
}

#[derive(Default)]
pub(super) struct Shared {
    finished: Mutex<Vec<Finished>>,
    changed: Condvar,
    /// Finished jobs not yet taken; lets the pre-run drain skip the mutex.
    ready: AtomicUsize,
    /// The worker thread's time (tests move it forward).
    clock: flush_mode::Clock,
}

impl Shared {
    /// Queue finished jobs for `complete_flushes` and wake the waiters. Never
    /// takes the platform lock, which a waiter may hold.
    pub(super) fn publish(&self, done: impl IntoIterator<Item = Finished>) {
        let mut finished = lock(&self.finished);
        finished.extend(done);
        self.ready.store(finished.len(), Ordering::Release);
        drop(finished);
        self.changed.notify_all();
    }
}

#[derive(Default)]
pub(crate) struct FlushWorker {
    /// The thread's inbox; `None` once a namespace change let the thread go,
    /// until the next FLUSH starts one on the current disks.
    jobs: Option<Sender<FlushJob>>,
    pub(super) shared: Arc<Shared>,
    /// Flushes, READs and WRITEs not yet taken by `take_finished`.
    pub(super) in_flight: usize,
    /// READs and WRITEs that the doorbell vCPU has not taken yet.
    pub(super) io: Vec<IoJob>,
}

impl std::fmt::Debug for FlushWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlushWorker")
            .field("in_flight", &self.in_flight)
            .field("io", &self.io.len())
            .finish_non_exhaustive()
    }
}

impl FlushWorker {
    /// Start a thread that syncs `disk` and `disk2` and queues its
    /// completions here.
    fn start(&mut self, disk: Option<File>, disk2: Option<File>) -> io::Result<()> {
        let (jobs, inbox) = channel::<FlushJob>();
        let worker = Arc::clone(&self.shared);
        std::thread::Builder::new()
            .name("nvme-flush".into())
            .spawn(move || {
                let clock = &worker.clock;
                let mut disks = [disk.map(DiskSync::new), disk2.map(DiskSync::new)];
                // When a disk owes the full sync after an ordered flush: run it then.
                let mut due: Option<Instant> = None;
                loop {
                    // Ends when the controller drops its sender.
                    let next = match due {
                        None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        Some(at) => inbox.recv_timeout(at.saturating_duration_since(clock.now())),
                    };
                    let stop = matches!(next, Err(RecvTimeoutError::Disconnected));
                    if let Ok(first) = next {
                        let mut batch = vec![first];
                        batch.extend(inbox.try_iter());
                        let sync = |disk: &mut Option<DiskSync>, needed: bool| {
                            !needed || disk.as_mut().is_some_and(|d| d.sync(clock).is_ok())
                        };
                        let ok = sync(&mut disks[0], batch.iter().any(|job| job.disk));
                        let ok2 = sync(&mut disks[1], batch.iter().any(|job| job.disk2));
                        worker.publish(batch.into_iter().map(|job| Finished {
                            status: if (!job.disk || ok) && (!job.disk2 || ok2) {
                                SC_SUCCESS
                            } else {
                                SC_INTERNAL_DEVICE_ERROR
                            },
                            sync_next: false,
                            job,
                        }));
                        if let Some(hook) = FLUSH_DONE_HOOK.get() {
                            hook();
                        }
                    }
                    // A disk's debt comes due even while flushes of the other
                    // disk keep the worker busy, and before the worker goes.
                    let now = clock.now();
                    if stop || due.is_some_and(|at| at <= now) {
                        due = None;
                        for disk in disks.iter_mut().flatten() {
                            disk.settle(clock);
                        }
                    }
                    if stop {
                        break;
                    }
                    if due.is_none() && disks.iter().flatten().any(DiskSync::owed) {
                        due = Some(now + flush_mode::DURABLE_WITHIN);
                    }
                }
            })?;
        self.jobs = Some(jobs);
        Ok(())
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A write-back raw file's duplicated handle, which syncs the same file.
fn sync_handle(backend: Option<&DiskBackend>) -> Option<File> {
    match backend? {
        DiskBackend::RawFile(disk) if disk.write_back => disk.file.try_clone().ok(),
        _ => None,
    }
}

#[cfg(test)]
impl NvmeController {
    /// Move the flush worker's clock forward, as if `by` had passed.
    pub(crate) fn advance_flush_clock(&self, by: std::time::Duration) {
        let worker = self.flush_worker.as_ref().expect("a flush worker");
        worker.shared.clock.advance(by);
    }
}

impl NvmeController {
    /// Live VMMs enable this; tests and checkpointing keep the inline sync.
    pub fn set_async_flush(&mut self, enabled: bool) {
        self.async_flush = enabled;
    }

    /// Hand an I/O-queue FLUSH to the worker. `false` leaves it to the inline
    /// path: async off, an invalid NSID (its error status), a namespace with
    /// nothing to sync, or a worker that cannot start.
    pub(crate) fn defer_flush(&mut self, sqid: u16, cqid: u16, cmd: &SubmissionEntry) -> bool {
        if !self.async_flush || sqid == 0 {
            return false;
        }
        let (wants, wants2) = match cmd.nsid {
            u32::MAX => (true, true),
            NSID => (true, false),
            NSID2 if self.disk2.is_some() => (false, true),
            _ => return false,
        };
        // Only a write-back raw file has anything to sync; the others flush
        // inline as a no-op.
        let syncs = |backend: Option<&DiskBackend>| matches!(backend, Some(DiskBackend::RawFile(d)) if d.write_back);
        let disk = wants && syncs(Some(&self.disk));
        let disk2 = wants2 && syncs(self.disk2.as_ref());
        if !disk && !disk2 {
            return false;
        }
        if self
            .flush_worker
            .as_ref()
            .is_none_or(|worker| worker.jobs.is_none())
        {
            let (disk, disk2) = (
                sync_handle(Some(&self.disk)),
                sync_handle(self.disk2.as_ref()),
            );
            let worker = self.flush_worker.get_or_insert_with(FlushWorker::default);
            if let Err(error) = worker.start(disk, disk2) {
                eprintln!("nvme: flush worker not started, flushing inline: {error}");
                self.async_flush = false;
                return false;
            }
        }
        let job = FlushJob {
            sqid,
            cqid,
            cmd: *cmd,
            disk,
            disk2,
        };
        let Some(worker) = self.flush_worker.as_mut() else {
            return false;
        };
        if worker
            .jobs
            .as_ref()
            .is_none_or(|jobs| jobs.send(job).is_err())
        {
            self.flush_worker = None;
            self.async_flush = false;
            return false;
        }
        worker.in_flight += 1;
        self.hold_completion_slot(cqid);
        true
    }

    /// Keep a completion slot on `cqid` for a job in flight.
    pub(super) fn hold_completion_slot(&mut self, cqid: u16) {
        let slot = usize::from(cqid);
        if self.flush_reserved.len() <= slot {
            self.flush_reserved.resize(slot + 1, 0);
        }
        self.flush_reserved[slot] += 1;
    }

    /// The worker thread holds the disk handles it started with: a namespace
    /// change waits for its flushes and lets it go (it first runs the full
    /// syncs its disks still owe); the next FLUSH starts a thread on the
    /// current disks. Completions not yet posted stay queued.
    pub(crate) fn restart_flush_worker(&mut self) {
        self.wait_for_flushes();
        if let Some(worker) = self.flush_worker.as_mut() {
            worker.jobs = None;
        }
    }

    /// Completion slots held for flushes in flight on `cqid`.
    pub(crate) fn flush_reserved_slots(&self, cqid: u16) -> u16 {
        self.flush_reserved
            .get(usize::from(cqid))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn flushes_in_flight(&self) -> bool {
        self.flush_worker
            .as_ref()
            .is_some_and(|worker| worker.in_flight > 0)
    }

    /// True when finished flushes wait for `complete_flushes`.
    pub fn flushes_ready(&self) -> bool {
        self.flush_worker
            .as_ref()
            .is_some_and(|worker| worker.shared.ready.load(Ordering::Acquire) > 0)
    }

    /// Post the completions of finished flushes, READs and WRITEs. With
    /// `wait`, first wait until none is in flight, and return only when none
    /// is: a finished FUA write goes on to its sync.
    pub fn complete_flushes(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
        wait: bool,
    ) {
        loop {
            self.post_finished(mem, completions, wait);
            if !wait || !self.flushes_in_flight() {
                return;
            }
        }
    }

    fn post_finished(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
        wait: bool,
    ) {
        for done in self.take_finished(wait) {
            let FlushJob {
                sqid, cqid, cmd, ..
            } = done.job;
            // The data is in the file; the sync and then the completion follow,
            // as for the inline path (queue.rs).
            if done.sync_next && self.defer_flush(sqid, cqid, &cmd) {
                continue;
            }
            let status = if done.sync_next {
                self.io_flush(&cmd)
            } else {
                done.status
            };
            let queues_live = matches!(self.sqs.get(usize::from(sqid)), Some(Some(_)))
                && matches!(self.cqs.get(usize::from(cqid)), Some(Some(_)));
            let (posted, completion) = if queues_live {
                self.post_completion(cqid, sqid, &cmd, status, mem)
            } else {
                (false, None)
            };
            self.record_command_trace(NvmeCommandTrace {
                sqid,
                cqid,
                sq_head: 0,
                sq_tail: 0,
                sq_entry_gpa: 0,
                opcode: cmd.opcode,
                command_id: cmd.command_id,
                nsid: cmd.nsid,
                prp1: cmd.prp1,
                prp2: cmd.prp2,
                cdw10: cmd.cdw10,
                cdw11: cmd.cdw11,
                cdw12: cmd.cdw12,
                cdw13: cmd.cdw13,
                cdw14: cmd.cdw14,
                cdw15: cmd.cdw15,
                status,
                completion_posted: posted,
                completion: completion.map(|c| NvmeCompletionTrace {
                    cqid: c.cqid,
                    vector: c.vector,
                }),
            });
            completions.extend(completion);
        }
    }

    /// Wait for the flushes, READs and WRITEs in flight; their completions
    /// stay queued for `complete_flushes`.
    pub(crate) fn wait_for_flushes(&mut self) {
        self.run_untaken_io();
        let Some(worker) = self.flush_worker.as_ref() else {
            return;
        };
        let mut queue = lock(&worker.shared.finished);
        while queue.len() < worker.in_flight {
            queue = worker
                .shared
                .changed
                .wait(queue)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Wait for the flushes in flight and drop their completions: a reset
    /// takes the queues they would post to.
    pub(crate) fn discard_flushes(&mut self) {
        drop(self.take_finished(true));
    }

    fn take_finished(&mut self, wait: bool) -> Vec<Finished> {
        if wait {
            self.run_untaken_io();
        }
        let Some(worker) = self.flush_worker.as_mut() else {
            return Vec::new();
        };
        let mut queue = lock(&worker.shared.finished);
        while wait && queue.len() < worker.in_flight {
            queue = worker
                .shared
                .changed
                .wait(queue)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        let taken = std::mem::take(&mut *queue);
        worker.shared.ready.store(0, Ordering::Release);
        drop(queue);
        worker.in_flight -= taken.len();
        for done in &taken {
            if let Some(slot) = self.flush_reserved.get_mut(usize::from(done.job.cqid)) {
                *slot = slot.saturating_sub(1);
            }
        }
        taken
    }
}
