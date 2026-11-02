//! NVM FLUSH off the vCPU. A raw-file disk's `F_FULLFSYNC` takes about 1.5 ms
//! and tens of ms under load, and it ran inside the doorbell write, under the
//! platform lock, so every vCPU stalled behind each guest flush (guest timer
//! gaps up to 26 ms against 4-7 ms on QEMU/HVF while another thread flushed).
//!
//! With async flush on, an I/O-queue FLUSH for a write-back raw file goes to a
//! worker thread that syncs duplicated handles of the disk files, one sync per
//! disk for every flush queued meanwhile. The command completes, with the same
//! status rules, only after that sync returns: the pre-run drain posts the
//! completion, and `FLUSH_DONE_HOOK` asks a vCPU to run that drain at once.
//! FLUSH covers the writes completed before it was submitted (NVMe 1.4 6.8);
//! those are in the host page cache by then, so the later sync covers them.
//!
//! A controller register write, any admin command and a reset first wait for
//! the flushes in flight, so queue deletion and reset never meet a pending
//! completion; a reset drops their completions with the queues they targeted.

use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Called by the worker after each batch of syncs; the live VMM kicks a vCPU
/// so its pre-run drain posts the completions.
pub static FLUSH_DONE_HOOK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

#[derive(Clone, Copy)]
struct FlushJob {
    sqid: u16,
    cqid: u16,
    cmd: SubmissionEntry,
    disk: bool,
    disk2: bool,
}

struct Finished {
    job: FlushJob,
    ok: bool,
}

#[derive(Default)]
struct Shared {
    finished: Mutex<Vec<Finished>>,
    changed: Condvar,
    /// Finished jobs not yet taken; lets the pre-run drain skip the mutex.
    ready: AtomicUsize,
}

pub(crate) struct FlushWorker {
    jobs: Sender<FlushJob>,
    shared: Arc<Shared>,
    in_flight: usize,
}

impl std::fmt::Debug for FlushWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlushWorker")
            .field("in_flight", &self.in_flight)
            .finish_non_exhaustive()
    }
}

impl FlushWorker {
    fn spawn(disk: Option<File>, disk2: Option<File>) -> io::Result<Self> {
        let (jobs, inbox) = channel::<FlushJob>();
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("nvme-flush".into())
            .spawn(move || {
                // Ends when the controller drops its sender.
                while let Ok(first) = inbox.recv() {
                    let mut batch = vec![first];
                    batch.extend(inbox.try_iter());
                    let sync = |file: &Option<File>, needed: bool| {
                        !needed || file.as_ref().is_some_and(|f| flush_mode::sync(f).is_ok())
                    };
                    let ok = sync(&disk, batch.iter().any(|job| job.disk));
                    let ok2 = sync(&disk2, batch.iter().any(|job| job.disk2));
                    let done = batch.into_iter().map(|job| Finished {
                        ok: (!job.disk || ok) && (!job.disk2 || ok2),
                        job,
                    });
                    let mut finished = lock(&worker.finished);
                    finished.extend(done);
                    worker.ready.store(finished.len(), Ordering::Release);
                    drop(finished);
                    worker.changed.notify_all();
                    if let Some(hook) = FLUSH_DONE_HOOK.get() {
                        hook();
                    }
                }
            })?;
        Ok(Self {
            jobs,
            shared,
            in_flight: 0,
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
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
        if self.flush_worker.is_none() {
            let handles = (
                sync_handle(Some(&self.disk)),
                sync_handle(self.disk2.as_ref()),
            );
            match FlushWorker::spawn(handles.0, handles.1) {
                Ok(worker) => self.flush_worker = Some(worker),
                Err(error) => {
                    eprintln!("nvme: flush worker not started, flushing inline: {error}");
                    self.async_flush = false;
                    return false;
                }
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
        if worker.jobs.send(job).is_err() {
            self.flush_worker = None;
            self.async_flush = false;
            return false;
        }
        worker.in_flight += 1;
        let slot = usize::from(cqid);
        if self.flush_reserved.len() <= slot {
            self.flush_reserved.resize(slot + 1, 0);
        }
        self.flush_reserved[slot] += 1;
        true
    }

    /// The worker holds the disk handles it started with: a namespace change
    /// waits for its flushes and starts a new worker on the next FLUSH. Their
    /// completions, if any are still untaken, keep the current worker.
    pub(crate) fn restart_flush_worker(&mut self) {
        self.wait_for_flushes();
        if !self.flushes_ready() {
            self.flush_worker = None;
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

    /// Post the completions of finished flushes. With `wait`, first wait until
    /// none is in flight.
    pub fn complete_flushes(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
        wait: bool,
    ) {
        for done in self.take_finished(wait) {
            let FlushJob {
                sqid, cqid, cmd, ..
            } = done.job;
            let status = if done.ok {
                SC_SUCCESS
            } else {
                SC_INTERNAL_DEVICE_ERROR
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

    /// Wait for the flushes in flight; their completions stay queued for
    /// `complete_flushes`.
    pub(crate) fn wait_for_flushes(&self) {
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
