//! When the output AudioQueue pauses, without any CoreAudio call.
//!
//! A running queue keeps the Mac's output device running, and coreaudiod holds
//! a PreventUserIdleSystemSleep assertion for this process all the while, so a
//! VM whose guest played nothing kept the Mac from idle sleep. The supervisor
//! (hda_coreaudio_output.rs) pauses the queue once the guest has written no PCM
//! for between `IDLE_PAUSE` and twice that, and starts it again at the next
//! write. A pause keeps the queue's buffers and the ring, so nothing the guest
//! wrote is lost. A guest stream that stops has already flushed the ring by then
//! (hda_coreaudio_prefill.rs); one that runs writes PCM, silent or not, every poll.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::OnceLock;
use std::thread::Thread;
use std::time::{Duration, Instant};

pub(super) const IDLE_PAUSE: Duration = Duration::from_secs(1);

/// What the supervisor does to the queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Step {
    Pause,
    Start,
}

/// Pause after a quiet `IDLE_PAUSE`, start at the next write. The supervisor
/// sees the sink's write count only when it wakes, so a pause comes between
/// one and two `IDLE_PAUSE` after the last write.
pub(super) struct IdleGate {
    writes: u64,
    quiet_since: Instant,
    paused: bool,
}

impl IdleGate {
    /// A running queue, with `writes` PCM writes seen so far.
    pub(super) fn new(writes: u64, now: Instant) -> Self {
        Self {
            writes,
            quiet_since: now,
            paused: false,
        }
    }

    /// What the queue needs at `now`, given the sink's write count.
    pub(super) fn step(&mut self, writes: u64, now: Instant) -> Option<Step> {
        if writes != self.writes {
            self.writes = writes;
            self.quiet_since = now;
            return std::mem::take(&mut self.paused).then_some(Step::Start);
        }
        if self.paused || now.saturating_duration_since(self.quiet_since) < IDLE_PAUSE {
            return None;
        }
        self.paused = true;
        Some(Step::Pause)
    }

    /// How long the supervisor may sleep: until the queue could turn idle, or
    /// `None` while it is paused, since the next write wakes the supervisor.
    pub(super) fn sleep(&self, now: Instant) -> Option<Duration> {
        (!self.paused).then(|| (self.quiet_since + IDLE_PAUSE).saturating_duration_since(now))
    }
}

/// What the producer, the supervisor and the sink's teardown share. The
/// producer's part is one atomic add and one load per write, under the
/// platform lock; it unparks the supervisor only while the queue is paused.
#[derive(Default)]
pub(super) struct OutputControl {
    writes: AtomicU64,
    paused: AtomicBool,
    stop: AtomicBool,
    supervisor: OnceLock<Thread>,
}

impl OutputControl {
    /// The sink wrote guest PCM. Producer only.
    pub(super) fn note_pcm(&self) {
        self.writes.fetch_add(1, SeqCst);
        // Paired with `set_paused`: either the supervisor's next read of the
        // write count sees this write, or this load sees the pause.
        if self.paused.load(SeqCst) {
            self.wake();
        }
    }

    pub(super) fn writes(&self) -> u64 {
        self.writes.load(SeqCst)
    }

    /// Set before the queue pauses and before the supervisor reads the write
    /// count again; cleared before the queue starts.
    pub(super) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, SeqCst);
    }

    /// The thread that `wake` unparks. Set by the supervisor itself before its
    /// first pause, so a write that sees a pause always finds it.
    pub(super) fn register_supervisor(&self, thread: Thread) {
        let _ = self.supervisor.set(thread);
    }

    pub(super) fn wake(&self) {
        if let Some(thread) = self.supervisor.get() {
            thread.unpark();
        }
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, SeqCst);
        self.wake();
    }

    pub(super) fn stopping(&self) -> bool {
        self.stop.load(SeqCst)
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_idle_tests.rs"]
mod tests;
