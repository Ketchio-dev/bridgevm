//! AudioQueue's output callback. It refills each buffer the queue returns from
//! the ring without ever waiting on the vCPU producer, records what the buffer
//! received (hda_coreaudio_continuity.rs) and hands it back to the queue.

use std::ffi::c_void;
use std::ptr;

use super::hda_coreaudio_continuity::fill_and_record;
use super::hda_coreaudio_stats::Shared;
use super::{AudioQueueBuffer, AudioQueueEnqueueBuffer, CallbackContext};

pub(super) unsafe extern "C" fn output_callback(
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
    fill_and_record(destination, shared);
}

pub(super) unsafe fn fill_with_silence(buffer: *mut AudioQueueBuffer) {
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
