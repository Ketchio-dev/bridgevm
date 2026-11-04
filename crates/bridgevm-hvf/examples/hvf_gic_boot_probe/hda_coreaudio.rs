//! CoreAudio output sink for the live HVF probe.
//!
//! HDA polling calls `write_pcm` while the platform lock is held. The owning
//! sink's `&mut HdaPcmSink` calls serialize its sole producer. The callback
//! context only owns shared PCM state and cannot publish frames. Complete
//! frames cross an atomic ring; a non-waiting consumer guard protects overlapping
//! callbacks, which otherwise substitute silence only on underrun. A supervisor
//! thread pauses the queue while the guest plays nothing, starts it at the
//! next write, and moves playback to a new queue on the Mac's current default
//! output when that changes or the queue fails (hda_coreaudio_output.rs).
//!
//! Teardown prints the `hda CoreAudio stats:` record, whose exact ordered
//! field set B7 and T17 parse, and after it a separate `hda CoreAudio
//! continuity:` record counting the silence substituted inside playback
//! (hda_coreaudio_continuity.rs), which the stats record cannot show.

use std::ffi::c_void;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[path = "hda_coreaudio_fill.rs"]
mod hda_coreaudio_fill;
#[path = "hda_coreaudio_frames.rs"]
mod hda_coreaudio_frames;
#[path = "hda_coreaudio_producer.rs"]
mod hda_coreaudio_producer;
use hda_coreaudio_producer::PcmProducer;
#[path = "hda_coreaudio_atomic_ring.rs"]
mod hda_coreaudio_atomic_ring;
#[path = "hda_coreaudio_callback.rs"]
mod hda_coreaudio_callback;
#[path = "hda_coreaudio_continuity.rs"]
mod hda_coreaudio_continuity;
#[path = "hda_coreaudio_idle.rs"]
mod hda_coreaudio_idle;
#[path = "hda_coreaudio_output.rs"]
mod hda_coreaudio_output;
use hda_coreaudio_output::{Output, OutputQueue};
#[path = "hda_coreaudio_prefill.rs"]
mod hda_coreaudio_prefill;
#[path = "hda_coreaudio_publication.rs"]
mod hda_coreaudio_publication;
#[path = "hda_coreaudio_rebuild.rs"]
mod hda_coreaudio_rebuild;
#[path = "hda_coreaudio_ring.rs"]
mod hda_coreaudio_ring;
#[path = "hda_coreaudio_start.rs"]
mod hda_coreaudio_start;
pub(crate) use hda_coreaudio_start::{start_output, ENABLED};
#[path = "hda_coreaudio_stats.rs"]
mod hda_coreaudio_stats;
#[path = "hda_coreaudio_teardown.rs"]
mod hda_coreaudio_teardown;
use hda_coreaudio_prefill::PREFILL_BYTES;
use hda_coreaudio_stats::Shared;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u8 = 2;
const BITS_PER_CHANNEL: u8 = 16;
const BYTES_PER_FRAME: u32 = 4;
const AUDIO_QUEUE_BUFFER_BYTES: u32 = 480 * BYTES_PER_FRAME; // 10 ms.
const AUDIO_QUEUE_BUFFER_COUNT: usize = 3;
const RING_CAPACITY_BYTES: usize = SAMPLE_RATE as usize * BYTES_PER_FRAME as usize / 2;

const AUDIO_FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER: u32 = 1 << 2;
const AUDIO_FORMAT_FLAG_IS_PACKED: u32 = 1 << 3;

type AudioQueueOutputCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut AudioQueueBuffer);

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioQueueBuffer {
    audio_data_bytes_capacity: u32,
    audio_data: *mut c_void,
    audio_data_byte_size: u32,
    user_data: *mut c_void,
    packet_description_capacity: u32,
    packet_descriptions: *mut c_void,
    packet_description_count: u32,
}

struct CallbackContext {
    shared: Arc<Shared>,
    /// Set before a replaced queue stops (hda_coreaudio_output.rs).
    retired: AtomicBool,
}

/// Fixed-format AudioQueue sink for the Windows HDA endpoint's s16le stream.
pub struct CoreAudioPcmSink {
    producer: PcmProducer,
    output: Output,
    shared: Arc<Shared>,
}

impl CoreAudioPcmSink {
    pub fn new() -> Result<Self, String> {
        let shared = Arc::new(Shared::new(RING_CAPACITY_BYTES, PREFILL_BYTES));
        let queue = OutputQueue::open(&shared)?;
        Ok(Self {
            producer: PcmProducer::default(),
            output: Output::supervise(queue, &shared),
            shared,
        })
    }
}

impl Drop for CoreAudioPcmSink {
    fn drop(&mut self) {
        // Account for a terminal fragment; immediate stop does not prove playback.
        self.producer.finish(&self.shared, RING_CAPACITY_BYTES);
        // No rebuild, pause or device notification from here on.
        self.output.end_supervision();
        self.shared.callback_failures.begin_stopping();
        self.shared.continuity.begin_stopping();
        let [stop_status, dispose_status] = self.output.close();
        // Always print the healthy case too; error-only telemetry cannot prove
        // A5's required frames_rendered>0 AND drops==0.
        self.shared.print_stats([stop_status, dispose_status]);
        println!("{}", self.shared.continuity.snapshot().record());
    }
}

fn status_error(operation: &str, status: i32) -> String {
    format!("{operation} failed with OSStatus {status} ({status:#010x})")
}

extern "C" {
    fn AudioQueueNewOutput(
        format: *const AudioStreamBasicDescription,
        callback: Option<AudioQueueOutputCallback>,
        user_data: *mut c_void,
        callback_run_loop: *mut c_void,
        callback_run_loop_mode: *const c_void,
        flags: u32,
        queue: *mut *mut c_void,
    ) -> i32;
    fn AudioQueueAllocateBuffer(
        queue: *mut c_void,
        buffer_byte_size: u32,
        buffer: *mut *mut AudioQueueBuffer,
    ) -> i32;
    fn AudioQueueEnqueueBuffer(
        queue: *mut c_void,
        buffer: *mut AudioQueueBuffer,
        packet_description_count: u32,
        packet_descriptions: *const c_void,
    ) -> i32;
    fn AudioQueueStart(queue: *mut c_void, start_time: *const c_void) -> i32;
    fn AudioQueuePause(queue: *mut c_void) -> i32;
    fn AudioQueueStop(queue: *mut c_void, immediate: u8) -> i32;
    fn AudioQueueDispose(queue: *mut c_void, immediate: u8) -> i32;
    fn AudioQueueGetProperty(
        queue: *mut c_void,
        property: u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}
