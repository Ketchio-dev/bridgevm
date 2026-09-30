//! Playback-continuity counters for the CoreAudio output callback.
//!
//! The callback fills each 10 ms AudioQueue buffer from the ring and leaves
//! silence wherever the ring cannot: the ring holds fewer bytes than the
//! buffer (an underrun, partial or whole), or the producer holds the ring lock
//! and try_lock fails (contention, a whole silent buffer). frames_rendered and
//! drops cannot see either, so A5 and T17 pass on gapped playback. These
//! counters make the gaps measurable; teardown prints them as their own
//! `hda CoreAudio continuity:` record after the unchanged stats record.
//!
//! Silence counts only inside a playback span, and only once later guest PCM
//! in the same span proves it was a gap rather than the end of playback:
//!
//! - a span opens at the first callback that delivers guest PCM, so warm-up
//!   silence before the guest streams is never counted;
//! - a span closes when a callback leaves the ring short after the guest
//!   stopped its stream (`HdaPcmSink::stream_stopped`) and wrote no PCM since,
//!   and at teardown; silence after a span's last PCM is never counted.
//!
//! Record fields, all counted inside spans before teardown began:
//!
//! - `active_callbacks`: callbacks from a span's first PCM-delivering callback
//!   through its last one;
//! - `underrun_callbacks`: those that substituted any silence that later PCM
//!   followed, partial or whole buffers, contention included;
//! - `underrun_frames`: the silent frames they substituted;
//! - `contention_callbacks`: underrun callbacks left wholly silent because
//!   try_lock found the producer holding the ring;
//! - `gaps`: maximal silent runs, each ended by guest PCM;
//! - `max_gap_frames`: the longest such run in frames;
//! - `stream_stops`: guest stream stops reported to the sink, at any time;
//! - `callback_frames`: frames per callback buffer (480, 10 ms at 48 kHz).
//!
//! Only the callback thread changes span state and the counted tallies; the
//! producer only marks the stream fed or stopped. Every update is a relaxed
//! atomic, so the real-time thread never allocates, locks or waits for them.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::TryLockError;

use super::hda_coreaudio_ring::drain_ring_into;
use super::hda_coreaudio_stats::Shared;
use super::{AUDIO_QUEUE_BUFFER_BYTES, BYTES_PER_FRAME};

const PREFIX: &str = "hda CoreAudio continuity:";
const FRAME_BYTES: usize = BYTES_PER_FRAME as usize;
const CALLBACK_FRAMES: u64 = (AUDIO_QUEUE_BUFFER_BYTES / BYTES_PER_FRAME) as u64;

/// What one output callback put into its already-silent buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallbackFill {
    /// try_lock found the producer holding the ring; the buffer stays silent.
    Contended { capacity_bytes: usize },
    /// The ring supplied `pcm_bytes`; `stream_idle` was read under the ring lock.
    Drained {
        pcm_bytes: usize,
        capacity_bytes: usize,
        stream_idle: bool,
    },
}

/// Fill one silent callback buffer from the ring without waiting, and record
/// what it received. Runs on CoreAudio's real-time callback thread.
pub(super) fn fill_and_record(destination: &mut [u8], shared: &Shared) {
    let capacity_bytes = destination.len();
    let mut ring = match shared.ring.try_lock() {
        Ok(ring) => ring,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return shared
                .continuity
                .record(CallbackFill::Contended { capacity_bytes });
        }
    };
    let pcm_bytes = drain_ring_into(&mut ring, destination);
    let stream_idle = shared.continuity.stream_idle();
    drop(ring);
    let fill = CallbackFill::Drained {
        pcm_bytes,
        capacity_bytes,
        stream_idle,
    };
    shared.continuity.record(fill);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    callbacks: u64,
    underrun_callbacks: u64,
    underrun_frames: u64,
    contention_callbacks: u64,
}

/// Callback and silence tallies. `pending` holds silence no later PCM has yet
/// proven to be a gap; `committed` is what the record reports.
#[derive(Default)]
struct Tally {
    callbacks: AtomicU64,
    underrun_callbacks: AtomicU64,
    underrun_frames: AtomicU64,
    contention_callbacks: AtomicU64,
}

impl Tally {
    fn add(&self, counts: Counts) {
        self.callbacks.fetch_add(counts.callbacks, Relaxed);
        self.underrun_callbacks
            .fetch_add(counts.underrun_callbacks, Relaxed);
        self.underrun_frames
            .fetch_add(counts.underrun_frames, Relaxed);
        self.contention_callbacks
            .fetch_add(counts.contention_callbacks, Relaxed);
    }

    fn take(&self) -> Counts {
        Counts {
            callbacks: self.callbacks.swap(0, Relaxed),
            underrun_callbacks: self.underrun_callbacks.swap(0, Relaxed),
            underrun_frames: self.underrun_frames.swap(0, Relaxed),
            contention_callbacks: self.contention_callbacks.swap(0, Relaxed),
        }
    }

    fn load(&self) -> Counts {
        Counts {
            callbacks: self.callbacks.load(Relaxed),
            underrun_callbacks: self.underrun_callbacks.load(Relaxed),
            underrun_frames: self.underrun_frames.load(Relaxed),
            contention_callbacks: self.contention_callbacks.load(Relaxed),
        }
    }
}

#[derive(Default)]
pub(super) struct ContinuityCounters {
    stopping: AtomicBool,
    /// Guest PCM reached the ring since the guest last stopped its stream.
    /// Written and read under the ring lock, except that a stop may land any
    /// time: the stream has then stopped, whichever PCM the ring still holds.
    streaming: AtomicBool,
    in_span: AtomicBool,
    committed: Tally,
    pending: Tally,
    gaps: AtomicU64,
    max_gap_frames: AtomicU64,
    stream_stops: AtomicU64,
}

impl ContinuityCounters {
    /// The producer stored guest PCM in the ring and still holds its lock.
    pub(super) fn note_pcm_written(&self) {
        self.streaming.store(true, Relaxed);
    }

    /// The guest stopped its playback stream (HdaPcmSink::stream_stopped).
    pub(super) fn note_stream_stopped(&self) {
        self.streaming.store(false, Relaxed);
        self.stream_stops.fetch_add(1, Relaxed);
    }

    pub(super) fn stream_idle(&self) -> bool {
        !self.streaming.load(Relaxed)
    }

    /// Teardown began: later callbacks and still-pending silence never count.
    pub(super) fn begin_stopping(&self) {
        self.stopping.store(true, Relaxed);
    }

    /// Count one output callback. Callback thread only.
    pub(super) fn record(&self, fill: CallbackFill) {
        if self.stopping.load(Relaxed) {
            return;
        }
        let (pcm_bytes, capacity_bytes, stream_idle, contended) = match fill {
            CallbackFill::Contended { capacity_bytes } => (0, capacity_bytes, false, true),
            CallbackFill::Drained {
                pcm_bytes,
                capacity_bytes,
                stream_idle,
            } => (pcm_bytes, capacity_bytes, stream_idle, false),
        };
        if pcm_bytes > 0 {
            // Guest PCM follows the pending silence, so that silence was a gap.
            self.commit_pending();
            self.committed.add(Counts {
                callbacks: 1,
                ..Counts::default()
            });
            self.in_span.store(true, Relaxed);
        } else if self.in_span.load(Relaxed) {
            self.pending.add(Counts {
                callbacks: 1,
                ..Counts::default()
            });
        } else {
            return;
        }
        let silent_frames = capacity_bytes
            .saturating_sub(pcm_bytes)
            .div_ceil(FRAME_BYTES) as u64;
        if silent_frames == 0 {
            return;
        }
        if stream_idle {
            // The guest stopped its stream and the ring ran short: the span ends.
            self.pending.take();
            self.in_span.store(false, Relaxed);
            return;
        }
        self.pending.add(Counts {
            callbacks: 0,
            underrun_callbacks: 1,
            underrun_frames: silent_frames,
            contention_callbacks: u64::from(contended),
        });
    }

    fn commit_pending(&self) {
        let gap = self.pending.take();
        if gap.underrun_frames > 0 {
            self.gaps.fetch_add(1, Relaxed);
            self.max_gap_frames.fetch_max(gap.underrun_frames, Relaxed);
        }
        self.committed.add(gap);
    }

    pub(super) fn snapshot(&self) -> ContinuitySnapshot {
        let counted = self.committed.load();
        ContinuitySnapshot {
            active_callbacks: counted.callbacks,
            underrun_callbacks: counted.underrun_callbacks,
            underrun_frames: counted.underrun_frames,
            contention_callbacks: counted.contention_callbacks,
            gaps: self.gaps.load(Relaxed),
            max_gap_frames: self.max_gap_frames.load(Relaxed),
            stream_stops: self.stream_stops.load(Relaxed),
            callback_frames: CALLBACK_FRAMES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ContinuitySnapshot {
    pub(super) active_callbacks: u64,
    pub(super) underrun_callbacks: u64,
    pub(super) underrun_frames: u64,
    pub(super) contention_callbacks: u64,
    pub(super) gaps: u64,
    pub(super) max_gap_frames: u64,
    pub(super) stream_stops: u64,
    pub(super) callback_frames: u64,
}

impl ContinuitySnapshot {
    /// The host record; scripts/live-gates/hvf_host_tail.py pins its grammar.
    pub(super) fn record(&self) -> String {
        format!(
            "{PREFIX} active_callbacks={} underrun_callbacks={} underrun_frames={} contention_callbacks={} gaps={} max_gap_frames={} stream_stops={} callback_frames={}",
            self.active_callbacks,
            self.underrun_callbacks,
            self.underrun_frames,
            self.contention_callbacks,
            self.gaps,
            self.max_gap_frames,
            self.stream_stops,
            self.callback_frames
        )
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_continuity_tests.rs"]
mod tests;
