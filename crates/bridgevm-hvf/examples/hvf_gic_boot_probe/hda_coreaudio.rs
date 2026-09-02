//! Non-blocking CoreAudio output sink for the live HVF probe.
//!
//! The HDA controller calls `write_pcm` on the vCPU thread while the platform
//! lock is held. That method only attempts a short ring-buffer lock and copies
//! bytes; AudioQueue's private callback thread drains the ring into its own
//! buffers and substitutes silence on underrun or lock contention.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, TryLockError};

use bridgevm_hvf::hda::HdaPcmSink;
#[path = "hda_coreaudio_ring.rs"]
mod hda_coreaudio_ring;
#[path = "hda_coreaudio_stats.rs"]
mod hda_coreaudio_stats;
#[path = "hda_coreaudio_teardown.rs"]
mod hda_coreaudio_teardown;
use hda_coreaudio_ring::drain_ring_into;
use hda_coreaudio_stats::Shared;
use hda_coreaudio_teardown::dispose_failed_queue;

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
}

/// Fixed-format AudioQueue sink for the Windows HDA endpoint's s16le stream.
pub struct CoreAudioPcmSink {
    queue: *mut c_void,
    callback_context: *mut CallbackContext,
    shared: Arc<Shared>,
}

// The queue is created once and subsequently touched only by CoreAudio and by
// this value's Drop implementation. Shared producer/callback state is synchronized.
unsafe impl Send for CoreAudioPcmSink {}

impl CoreAudioPcmSink {
    pub fn new() -> Result<Self, String> {
        let format = AudioStreamBasicDescription {
            sample_rate: f64::from(SAMPLE_RATE),
            format_id: AUDIO_FORMAT_LINEAR_PCM,
            format_flags: AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER | AUDIO_FORMAT_FLAG_IS_PACKED,
            bytes_per_packet: BYTES_PER_FRAME,
            frames_per_packet: 1,
            bytes_per_frame: BYTES_PER_FRAME,
            channels_per_frame: u32::from(CHANNELS),
            bits_per_channel: u32::from(BITS_PER_CHANNEL),
            reserved: 0,
        };
        let shared = Arc::new(Shared::new(RING_CAPACITY_BYTES));
        let callback_context = Box::into_raw(Box::new(CallbackContext {
            shared: Arc::clone(&shared),
        }));
        let mut queue = ptr::null_mut();
        let status = unsafe {
            AudioQueueNewOutput(
                &format,
                Some(output_callback),
                callback_context.cast(),
                ptr::null_mut(),
                ptr::null(),
                0,
                &mut queue,
            )
        };
        if status != 0 {
            unsafe { drop(Box::from_raw(callback_context)) };
            return Err(status_error("AudioQueueNewOutput", status));
        }

        for _ in 0..AUDIO_QUEUE_BUFFER_COUNT {
            let mut buffer = ptr::null_mut();
            let status =
                unsafe { AudioQueueAllocateBuffer(queue, AUDIO_QUEUE_BUFFER_BYTES, &mut buffer) };
            if status != 0 {
                unsafe { dispose_failed_queue(queue, callback_context, AudioQueueDispose) };
                return Err(status_error("AudioQueueAllocateBuffer", status));
            }
            unsafe { fill_with_silence(buffer) };
            let status = unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null()) };
            if status != 0 {
                unsafe { dispose_failed_queue(queue, callback_context, AudioQueueDispose) };
                return Err(status_error("AudioQueueEnqueueBuffer", status));
            }
        }

        let status = unsafe { AudioQueueStart(queue, ptr::null()) };
        if status != 0 {
            unsafe { dispose_failed_queue(queue, callback_context, AudioQueueDispose) };
            return Err(status_error("AudioQueueStart", status));
        }

        Ok(Self {
            queue,
            callback_context,
            shared,
        })
    }
}

impl HdaPcmSink for CoreAudioPcmSink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        if samples.is_empty() {
            return;
        }
        if rate != SAMPLE_RATE || channels != CHANNELS || bits != BITS_PER_CHANNEL {
            self.shared.record_format_drop(samples.len());
            return;
        }

        // The old producer-side try_lock discarded the entire DMA fragment
        // whenever CoreAudio's callback happened to hold this same short-lived
        // lock. A live run showed 54 such tiny drops totaling only 3,668 bytes.
        // Blocking here preserves PCM; the callback remains try_lock-based so
        // the real-time CoreAudio thread can always substitute silence instead
        // of waiting on the vCPU.
        let mut ring = match self.shared.ring.lock() {
            Ok(ring) => ring,
            Err(poisoned) => poisoned.into_inner(),
        };
        if samples.len() > RING_CAPACITY_BYTES.saturating_sub(ring.len()) {
            drop(ring);
            self.shared.record_ring_full_drop(samples.len());
            return;
        }
        ring.extend(samples.iter().copied());
        // Every earlier return records a drop. Increment only after the bytes
        // have actually reached the ring, and report audio frames rather than
        // bytes (s16 stereo = four bytes per frame).
        self.shared.frames_rendered.fetch_add(
            samples.len() as u64 / u64::from(BYTES_PER_FRAME),
            Ordering::Relaxed,
        );
    }
}

impl Drop for CoreAudioPcmSink {
    fn drop(&mut self) {
        self.shared.callback_failures.begin_stopping();
        let (stop_status, dispose_status) = unsafe {
            let stop = AudioQueueStop(self.queue, 1);
            let dispose = AudioQueueDispose(self.queue, 1);
            if dispose == 0 {
                drop(Box::from_raw(self.callback_context));
            }
            (stop, dispose)
        };
        // Always print the healthy case too; error-only telemetry cannot prove
        // A5's required frames_rendered>0 AND drops==0.
        self.shared.print_stats([stop_status, dispose_status]);
    }
}

unsafe extern "C" fn output_callback(
    user_data: *mut c_void,
    queue: *mut c_void,
    buffer: *mut AudioQueueBuffer,
) {
    if user_data.is_null() || buffer.is_null() {
        return;
    }
    let context = &*(user_data.cast::<CallbackContext>());
    fill_from_ring(buffer, &context.shared);
    let status = AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null());
    if status != 0 {
        context.shared.callback_failures.record(status);
    }
}

unsafe fn fill_from_ring(buffer: *mut AudioQueueBuffer, shared: &Shared) {
    fill_with_silence(buffer);
    let buffer = &mut *buffer;
    if buffer.audio_data.is_null() {
        return;
    }
    let capacity = buffer.audio_data_bytes_capacity as usize;
    let destination = std::slice::from_raw_parts_mut(buffer.audio_data.cast::<u8>(), capacity);
    let mut ring = match shared.ring.try_lock() {
        Ok(ring) => ring,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    let _ = drain_ring_into(&mut ring, destination);
}

unsafe fn fill_with_silence(buffer: *mut AudioQueueBuffer) {
    if buffer.is_null() {
        return;
    }
    let buffer = &mut *buffer;
    if !buffer.audio_data.is_null() {
        ptr::write_bytes(
            buffer.audio_data.cast::<u8>(),
            0,
            buffer.audio_data_bytes_capacity as usize,
        );
    }
    buffer.audio_data_byte_size = buffer.audio_data_bytes_capacity;
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
    fn AudioQueueStop(queue: *mut c_void, immediate: u8) -> i32;
    fn AudioQueueDispose(queue: *mut c_void, immediate: u8) -> i32;
}
