//! The Mac's default input as the guest microphone (BRIDGEVM_HDA_MICROPHONE=1).
//!
//! The controller calls this source under the platform lock, so there it only
//! moves bytes and flips flags. A supervisor thread owns the AudioQueue: it
//! opens an input queue on the Mac's current default input when the guest's
//! capture stream starts and closes it STOP_GRACE after the stream stops. A
//! guest that restarts capture within the grace keeps its queue, and the Mac's
//! microphone indicator turns off soon after a call ends. No AudioQueue call
//! runs under the platform lock.
//!
//! Captured PCM crosses an AtomicPcmRing of RING_BYTES. The guest reads only
//! once PRIME_BYTES are buffered, so 10 ms input callbacks and 5 ms guest polls
//! do not alternate audio with silence. An empty ring reads silence and primes
//! again. A ring above HIGH_WATER_BYTES drops its oldest audio down to the
//! prime level, which bounds the delay when the Mac's input clock runs ahead;
//! a full ring drops new input. A new capture stream starts from an empty ring.
//! A queue that cannot open queues one line, printed by the next guest read so
//! it comes before the final report, and the guest records silence.

use super::hda_coreaudio_atomic_ring::{AtomicPcmRing, Consumer};
use super::hda_coreaudio_publication::PcmQueue;
use bridgevm_hvf::hda::HdaPcmSource;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "hda_microphone_queue.rs"]
mod hda_microphone_queue;
pub(crate) use hda_microphone_queue::open_default_input;

const FRAME_BYTES: usize = 4;
const BYTES_PER_MS: usize = 48 * FRAME_BYTES;
const RING_BYTES: usize = 200 * BYTES_PER_MS;
const PRIME_BYTES: usize = 30 * BYTES_PER_MS;
const HIGH_WATER_BYTES: usize = 120 * BYTES_PER_MS;
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(1);

/// A started input queue; dropping it stops and disposes the queue.
pub(crate) type RunningInput = Box<dyn Send>;
/// Opens and starts an input queue that publishes into the ring.
pub(crate) type OpenInput = Box<dyn FnMut(Arc<CaptureRing>) -> Result<RunningInput, String> + Send>;

pub(crate) struct CaptureRing {
    ring: AtomicPcmRing,
}

impl CaptureRing {
    /// The input callback's publish: whole frames, dropped when the ring is full.
    pub(crate) fn publish(&self, pcm: &[u8]) {
        let whole = pcm.len() / FRAME_BYTES * FRAME_BYTES;
        let _ = self
            .ring
            .producer()
            .publish_parts(&pcm[..whole], &[], RING_BYTES);
    }
}

#[derive(Default)]
struct Control {
    capturing: AtomicBool,
    shutdown: AtomicBool,
}

pub(crate) struct MicrophoneSource {
    ring: Arc<CaptureRing>,
    control: Arc<Control>,
    notices: Arc<Mutex<Vec<String>>>,
    supervisor: Option<JoinHandle<()>>,
    primed: bool,
}

impl MicrophoneSource {
    pub(crate) fn new(open: OpenInput, grace: Duration) -> Result<Self, String> {
        let ring = Arc::new(CaptureRing {
            ring: AtomicPcmRing::new(RING_BYTES),
        });
        let control = Arc::new(Control::default());
        let notices = Arc::new(Mutex::new(Vec::new()));
        let supervisor = {
            let (ring, control, notices) = (ring.clone(), control.clone(), notices.clone());
            thread::Builder::new()
                .name("hda-microphone".into())
                .spawn(move || supervise(&control, &ring, open, grace, &notices))
                .map_err(|error| format!("supervisor thread: {error}"))?
        };
        Ok(Self {
            ring,
            control,
            notices,
            supervisor: Some(supervisor),
            primed: false,
        })
    }

    fn wake_supervisor(&self) {
        if let Some(supervisor) = &self.supervisor {
            supervisor.thread().unpark();
        }
    }
}

impl HdaPcmSource for MicrophoneSource {
    fn read_pcm(&mut self, out: &mut [u8], rate: u32, channels: u8, bits: u8) -> usize {
        if let Ok(mut notices) = self.notices.try_lock() {
            for line in notices.drain(..) {
                eprintln!("{line}");
            }
        }
        if (rate, channels, bits) != (48_000, 2, 16) {
            return 0;
        }
        let Some(mut consumer) = self.ring.ring.try_consumer() else {
            return 0;
        };
        let buffered = consumer.len();
        if buffered > HIGH_WATER_BYTES {
            discard(&mut consumer, buffered - PRIME_BYTES);
        }
        if !self.primed {
            if consumer.len() < PRIME_BYTES {
                return 0;
            }
            self.primed = true;
        }
        let filled = consumer.drain_into(out);
        if filled + FRAME_BYTES <= out.len() {
            self.primed = false; // ran dry: prime again
        }
        filled
    }

    fn stream_started(&mut self) {
        if let Some(mut consumer) = self.ring.ring.try_consumer() {
            let stale = consumer.len();
            discard(&mut consumer, stale);
        }
        self.primed = false;
        self.control.capturing.store(true, Ordering::Release);
        self.wake_supervisor();
    }

    fn stream_stopped(&mut self) {
        self.control.capturing.store(false, Ordering::Release);
        self.wake_supervisor();
    }
}

impl Drop for MicrophoneSource {
    /// Closes an open queue on the supervisor thread before returning. Prints
    /// nothing: the final report may already have ended.
    fn drop(&mut self) {
        self.control.shutdown.store(true, Ordering::Release);
        if let Some(supervisor) = self.supervisor.take() {
            supervisor.thread().unpark();
            let _ = supervisor.join();
        }
    }
}

fn discard(consumer: &mut Consumer<'_>, mut bytes: usize) {
    let mut scratch = [0u8; 4096];
    while bytes >= FRAME_BYTES {
        let drained = consumer.drain_into(&mut scratch[..bytes.min(4096)]);
        if drained == 0 {
            break;
        }
        bytes -= drained;
    }
}

fn supervise(
    control: &Control,
    ring: &Arc<CaptureRing>,
    mut open: OpenInput,
    grace: Duration,
    notices: &Mutex<Vec<String>>,
) {
    let mut input: Option<RunningInput> = None;
    let mut tried = false; // this capture stream already tried to open a queue
    let mut reported = false;
    let mut idle_since: Option<Instant> = None;
    while !control.shutdown.load(Ordering::Acquire) {
        if control.capturing.load(Ordering::Acquire) {
            idle_since = None;
            if input.is_none() && !std::mem::replace(&mut tried, true) {
                match open(Arc::clone(ring)) {
                    Ok(running) => input = Some(running),
                    Err(error) if !std::mem::replace(&mut reported, true) => {
                        notices.lock().unwrap().push(format!(
                            "HDA capture: the Mac microphone did not start ({error}); \
                             the guest microphone records silence"
                        ));
                    }
                    Err(_) => {}
                }
            }
            thread::park();
            continue;
        }
        tried = false;
        if input.is_none() {
            thread::park();
            continue;
        }
        let since = *idle_since.get_or_insert_with(Instant::now);
        match grace.checked_sub(since.elapsed()) {
            Some(left) if !left.is_zero() => thread::park_timeout(left),
            _ => {
                input = None;
                idle_since = None;
            }
        }
    }
}

#[cfg(test)]
#[path = "hda_microphone_tests.rs"]
mod tests;
