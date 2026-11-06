//! An AudioQueue input on the Mac's current default input: s16le 48 kHz
//! stereo in three 10 ms buffers. Only the microphone supervisor thread opens
//! and drops it; the callback runs on the queue's own thread.

use super::super::{
    status_error, AudioQueueAllocateBuffer, AudioQueueBuffer, AudioQueueDispose,
    AudioQueueEnqueueBuffer, AudioQueueStart, AudioQueueStop, AudioStreamBasicDescription,
    AUDIO_FORMAT_FLAG_IS_PACKED, AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER, AUDIO_FORMAT_LINEAR_PCM,
    AUDIO_QUEUE_BUFFER_BYTES, AUDIO_QUEUE_BUFFER_COUNT, BITS_PER_CHANNEL, BYTES_PER_FRAME,
    CHANNELS, SAMPLE_RATE,
};
use super::{CaptureRing, RunningInput};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

type AudioQueueInputCallback = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    *mut AudioQueueBuffer,
    *const c_void,
    u32,
    *const c_void,
);

struct InputContext {
    ring: Arc<CaptureRing>,
    stopping: AtomicBool,
}

struct InputQueue {
    queue: *mut c_void,
    context: *mut InputContext,
}

// The supervisor thread alone opens, holds and drops the queue; the callback
// reaches the context, whose fields are Sync, only through the queue.
unsafe impl Send for InputQueue {}

impl Drop for InputQueue {
    fn drop(&mut self) {
        unsafe {
            (*self.context).stopping.store(true, Ordering::Release);
            AudioQueueStop(self.queue, 1);
            // A synchronous dispose ends the callbacks; only then is the context free.
            if AudioQueueDispose(self.queue, 1) == 0 {
                drop(Box::from_raw(self.context));
            }
        }
    }
}

pub(crate) fn open_default_input(ring: Arc<CaptureRing>) -> Result<RunningInput, String> {
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
    let context = Box::into_raw(Box::new(InputContext {
        ring,
        stopping: AtomicBool::new(false),
    }));
    let mut queue = ptr::null_mut();
    let status = unsafe {
        AudioQueueNewInput(
            &format,
            Some(input_callback),
            context.cast(),
            ptr::null_mut(),
            ptr::null(),
            0,
            &mut queue,
        )
    };
    if status != 0 {
        unsafe { drop(Box::from_raw(context)) };
        return Err(status_error("AudioQueueNewInput", status));
    }
    // From here a failure drops `input`, which disposes the queue.
    let input = InputQueue { queue, context };
    for _ in 0..AUDIO_QUEUE_BUFFER_COUNT {
        let mut buffer = ptr::null_mut();
        let status =
            unsafe { AudioQueueAllocateBuffer(queue, AUDIO_QUEUE_BUFFER_BYTES, &mut buffer) };
        if status != 0 {
            return Err(status_error("AudioQueueAllocateBuffer", status));
        }
        let status = unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null()) };
        if status != 0 {
            return Err(status_error("AudioQueueEnqueueBuffer", status));
        }
    }
    let status = unsafe { AudioQueueStart(queue, ptr::null()) };
    if status != 0 {
        return Err(status_error("AudioQueueStart", status));
    }
    Ok(Box::new(input))
}

unsafe extern "C" fn input_callback(
    user_data: *mut c_void,
    queue: *mut c_void,
    buffer: *mut AudioQueueBuffer,
    _start_time: *const c_void,
    _packet_count: u32,
    _packet_descriptions: *const c_void,
) {
    let context = &*(user_data as *const InputContext);
    let bytes = (*buffer).audio_data_byte_size as usize;
    if bytes > 0 && !(*buffer).audio_data.is_null() {
        let pcm = std::slice::from_raw_parts((*buffer).audio_data as *const u8, bytes);
        context.ring.publish(pcm);
    }
    if !context.stopping.load(Ordering::Acquire) {
        AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null());
    }
}

extern "C" {
    fn AudioQueueNewInput(
        format: *const AudioStreamBasicDescription,
        callback: Option<AudioQueueInputCallback>,
        user_data: *mut c_void,
        callback_run_loop: *mut c_void,
        callback_run_loop_mode: *const c_void,
        flags: u32,
        queue: *mut *mut c_void,
    ) -> i32;
}
