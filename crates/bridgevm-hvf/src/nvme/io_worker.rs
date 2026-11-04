//! One vCPU's NVMe transfer thread. Under host disk load a raw-file `pread`
//! or `pwrite` takes up to about 1 s, and while the doorbell vCPU ran it
//! itself (unlocked_io.rs) that vCPU was out of the guest, so an interrupt
//! routed to it waited for the transfer. In PR #489's rerun R2 the HDA MSI was
//! posted on time but the guest cleared its status 20-350 ms later, in 19-208
//! episodes per run that held 97-99% of the stale audio repeats.
//!
//! The vCPU now hands its transfers to its own worker and waits at most
//! [`NVME_IO_WAIT`], spinning briefly first. Transfers that finish in time
//! complete as before: the vCPU's second hold posts them. Others stay with
//! the worker and the vCPU returns to the guest; the worker publishes their
//! results to the flush worker's `finished` list and calls `FLUSH_DONE_HOOK`,
//! whose vCPU drain posts them under the lock, as for an async FLUSH. Either
//! way they stay counted in flight, with their CQ slots held, until a drain
//! takes them, so every reset, shutdown and admin-command wait covers them.
//!
//! The thread owns a view of guest RAM. The VMM declares each worker after
//! the RAM backing, so dropping it, which joins the thread once it has run the
//! batches it was given, comes before the RAM is unmapped.

use std::sync::atomic::{AtomicU8, Ordering::SeqCst};
use std::sync::mpsc::{channel, Receiver, SendError, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::flush_worker::{lock, FLUSH_DONE_HOOK};
use super::unlocked_io::NvmeIo;
use crate::fwcfg::GuestMemoryMut;

/// How long a doorbell vCPU waits for its transfers before it returns to the
/// guest: a tenth of the 10 ms period of Windows' HDA buffer entries.
pub const NVME_IO_WAIT: Duration = Duration::from_millis(1);

/// The vCPU spins this long for its transfers before it parks, and the worker
/// for its next batch: a 4 KiB transfer from the host page cache plus a thread
/// wakeup, so a quick transfer pays no wakeup on either side.
const SPIN: Duration = Duration::from_micros(100);

const PENDING: u8 = 0;
const DONE: u8 = 1;
const DETACHED: u8 = 2;

/// Where one batch stands, shared by its vCPU and the worker.
#[derive(Default)]
struct Handoff {
    phase: AtomicU8,
    lock: Mutex<()>,
    done: Condvar,
}

impl Handoff {
    /// The worker published the batch. True when its vCPU had already gone
    /// back to the guest, so a drain must be asked for.
    fn finish(&self) -> bool {
        if self
            .phase
            .compare_exchange(PENDING, DONE, SeqCst, SeqCst)
            .is_err()
        {
            return true;
        }
        let _parked = lock(&self.lock);
        self.done.notify_one();
        false
    }

    /// Wait for the batch until `deadline`, spinning until `spin_until`. True
    /// when it was published in time.
    fn wait(&self, spin_until: Instant, deadline: Instant) -> bool {
        while Instant::now() < spin_until {
            if self.phase.load(SeqCst) == DONE {
                return true;
            }
            std::hint::spin_loop();
        }
        let mut parked = lock(&self.lock);
        while self.phase.load(SeqCst) == PENDING {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                // Published meanwhile: then it is in time after all.
                return self
                    .phase
                    .compare_exchange(PENDING, DETACHED, SeqCst, SeqCst)
                    .is_err();
            }
            parked = self
                .done
                .wait_timeout(parked, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

struct Batch {
    io: NvmeIo,
    handoff: Arc<Handoff>,
}

type Memory = Box<dyn FnOnce() -> Box<dyn GuestMemoryMut> + Send>;

/// One vCPU's transfer thread, started with its first batch.
pub struct NvmeIoWorker {
    name: String,
    memory: Option<Memory>,
    inbox: Option<Sender<Batch>>,
    thread: Option<JoinHandle<()>>,
}

impl NvmeIoWorker {
    /// `memory` runs on the worker thread and returns its view of the guest
    /// RAM the vCPU sees, which must stay mapped until the worker is dropped.
    pub fn new<M: GuestMemoryMut + 'static>(
        name: impl Into<String>,
        memory: impl FnOnce() -> M + Send + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            memory: Some(Box::new(move || Box::new(memory()))),
            inbox: None,
            thread: None,
        }
    }

    /// Run `io`'s transfers on the worker and wait up to `wait` for them.
    /// True: they finished, and the caller's drain under the lock posts them
    /// now. False: the caller goes back to the guest; the worker publishes
    /// them later and calls `FLUSH_DONE_HOOK`. Without a worker thread (it
    /// could not start) they run here, in `mem`, as before.
    pub fn run(&mut self, io: NvmeIo, mem: &dyn GuestMemoryMut, wait: Duration) -> bool {
        let start = Instant::now();
        if self.inbox.is_none() {
            self.start();
        }
        let handoff = Arc::new(Handoff::default());
        let batch = Batch {
            io,
            handoff: Arc::clone(&handoff),
        };
        let sent = match self.inbox.as_ref() {
            Some(inbox) => inbox.send(batch),
            None => Err(SendError(batch)),
        };
        if let Err(SendError(batch)) = sent {
            self.inbox = None;
            batch.io.run(mem);
            return true;
        }
        handoff.wait(start + SPIN.min(wait), start + wait)
    }

    fn start(&mut self) {
        let Some(memory) = self.memory.take() else {
            return;
        };
        let (inbox, batches) = channel();
        let spawned = std::thread::Builder::new()
            .name(self.name.clone())
            .spawn(move || serve(&*memory(), &batches));
        match spawned {
            Ok(thread) => {
                self.inbox = Some(inbox);
                self.thread = Some(thread);
            }
            Err(error) => {
                eprintln!(
                    "nvme: {} not started, transfers run on the vCPU: {error}",
                    self.name
                );
            }
        }
    }
}

fn serve(mem: &dyn GuestMemoryMut, batches: &Receiver<Batch>) {
    while let Some(Batch { io, handoff }) = next(batches) {
        io.run(mem);
        if handoff.finish() {
            if let Some(hook) = FLUSH_DONE_HOOK.get() {
                hook();
            }
        }
    }
}

/// The next batch, after a short spin and then parked; `None` once the
/// worker's owner has let it go.
fn next(batches: &Receiver<Batch>) -> Option<Batch> {
    let spin_until = Instant::now() + SPIN;
    loop {
        match batches.try_recv() {
            Ok(batch) => return Some(batch),
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) if Instant::now() < spin_until => std::hint::spin_loop(),
            Err(TryRecvError::Empty) => return batches.recv().ok(),
        }
    }
}

impl Drop for NvmeIoWorker {
    /// The thread runs the batches it was given, then ends.
    fn drop(&mut self) {
        drop(self.inbox.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
