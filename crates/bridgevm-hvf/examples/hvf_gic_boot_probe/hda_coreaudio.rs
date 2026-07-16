//! Non-blocking CoreAudio output sink for the live HVF probe.
//!
//! The HDA controller calls `write_pcm` on the vCPU thread while the platform
//! lock is held. That method only attempts a short ring-buffer lock and copies
//! bytes; AudioQueue's private callback thread drains the ring into its own
//! buffers and substitutes silence on underrun or lock contention.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

use bridgevm_hvf::hda::HdaPcmSink;

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

type OsStatus = i32;
type AudioQueueRef = *mut c_void;
type AudioQueueBufferRef = *mut AudioQueueBuffer;
type AudioQueueOutputCallback =
    unsafe extern "C" fn(*mut c_void, AudioQueueRef, AudioQueueBufferRef);

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

struct Shared {
    ring: Mutex<VecDeque<u8>>,
    dropped_writes: AtomicU64,
    dropped_bytes: AtomicU64,
    callback_errors: AtomicU64,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY_BYTES)),
            dropped_writes: AtomicU64::new(0),
            dropped_bytes: AtomicU64::new(0),
            callback_errors: AtomicU64::new(0),
        }
    }

    fn record_drop(&self, bytes: usize) {
        self.dropped_writes.fetch_add(1, Ordering::Relaxed);
        self.dropped_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

struct CallbackContext {
    shared: Arc<Shared>,
}

/// Fixed-format AudioQueue sink for the Windows HDA endpoint's s16le stream.
pub struct CoreAudioPcmSink {
    queue: AudioQueueRef,
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
        let shared = Arc::new(Shared::new());
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
                unsafe { dispose_failed_queue(queue, callback_context) };
                return Err(status_error("AudioQueueAllocateBuffer", status));
            }
            unsafe { fill_with_silence(buffer) };
            let status = unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null()) };
            if status != 0 {
                unsafe { dispose_failed_queue(queue, callback_context) };
                return Err(status_error("AudioQueueEnqueueBuffer", status));
            }
        }

        let status = unsafe { AudioQueueStart(queue, ptr::null()) };
        if status != 0 {
            unsafe { dispose_failed_queue(queue, callback_context) };
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
            self.shared.record_drop(samples.len());
            return;
        }

        let mut ring = match self.shared.ring.try_lock() {
            Ok(ring) => ring,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => {
                self.shared.record_drop(samples.len());
                return;
            }
        };
        if samples.len() > RING_CAPACITY_BYTES.saturating_sub(ring.len()) {
            drop(ring);
            self.shared.record_drop(samples.len());
            return;
        }
        ring.extend(samples.iter().copied());
    }
}

impl Drop for CoreAudioPcmSink {
    fn drop(&mut self) {
        unsafe {
            let _ = AudioQueueStop(self.queue, 1);
            let _ = AudioQueueDispose(self.queue, 1);
            drop(Box::from_raw(self.callback_context));
        }
        let dropped_writes = self.shared.dropped_writes.load(Ordering::Relaxed);
        let callback_errors = self.shared.callback_errors.load(Ordering::Relaxed);
        if dropped_writes != 0 || callback_errors != 0 {
            eprintln!(
                "hda CoreAudio: dropped_writes={} dropped_bytes={} callback_errors={}",
                dropped_writes,
                self.shared.dropped_bytes.load(Ordering::Relaxed),
                callback_errors
            );
        }
    }
}

unsafe extern "C" fn output_callback(
    user_data: *mut c_void,
    queue: AudioQueueRef,
    buffer: AudioQueueBufferRef,
) {
    if user_data.is_null() || buffer.is_null() {
        return;
    }
    let context = &*(user_data.cast::<CallbackContext>());
    fill_from_ring(buffer, &context.shared);
    let status = AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null());
    if status != 0 {
        context
            .shared
            .callback_errors
            .fetch_add(1, Ordering::Relaxed);
    }
}

unsafe fn fill_from_ring(buffer: AudioQueueBufferRef, shared: &Shared) {
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
    for byte in destination.iter_mut().take(ring.len().min(capacity)) {
        *byte = ring.pop_front().unwrap();
    }
}

unsafe fn fill_with_silence(buffer: AudioQueueBufferRef) {
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

unsafe fn dispose_failed_queue(queue: AudioQueueRef, callback_context: *mut CallbackContext) {
    let _ = AudioQueueDispose(queue, 1);
    drop(Box::from_raw(callback_context));
}

fn status_error(operation: &str, status: OsStatus) -> String {
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
        queue: *mut AudioQueueRef,
    ) -> OsStatus;
    fn AudioQueueAllocateBuffer(
        queue: AudioQueueRef,
        buffer_byte_size: u32,
        buffer: *mut AudioQueueBufferRef,
    ) -> OsStatus;
    fn AudioQueueEnqueueBuffer(
        queue: AudioQueueRef,
        buffer: AudioQueueBufferRef,
        packet_description_count: u32,
        packet_descriptions: *const c_void,
    ) -> OsStatus;
    fn AudioQueueStart(queue: AudioQueueRef, start_time: *const c_void) -> OsStatus;
    fn AudioQueueStop(queue: AudioQueueRef, immediate: u8) -> OsStatus;
    fn AudioQueueDispose(queue: AudioQueueRef, immediate: u8) -> OsStatus;
}
