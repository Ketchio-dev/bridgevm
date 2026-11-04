//! The sink's AudioQueue and the supervisor thread that pauses and starts it.
//!
//! The producer only counts writes (hda_coreaudio_idle.rs). Every AudioQueue
//! call runs on the supervisor, or at teardown on the dropping thread once the
//! supervisor has ended, never under the platform lock. Pausing and starting
//! print nothing: a line printed after the final report's footer would end
//! every reader's host tail (scripts/live-gates/hvf_host_tail.py).
//! On macOS 27.0.1 coreaudiod dropped its idle-sleep assertion about 8 s
//! after a pause, and 3 s after a stop or dispose.

use std::ffi::c_void;
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::hda_coreaudio_callback::{fill_with_silence, output_callback};
use super::hda_coreaudio_idle::{IdleGate, Step};
use super::hda_coreaudio_stats::Shared;
use super::hda_coreaudio_teardown::dispose_failed_queue;
use super::{
    status_error, AudioQueueAllocateBuffer, AudioQueueDispose, AudioQueueEnqueueBuffer,
    AudioQueueNewOutput, AudioQueuePause, AudioQueueStart, AudioQueueStop,
    AudioStreamBasicDescription, CallbackContext, AUDIO_FORMAT_FLAG_IS_PACKED,
    AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER, AUDIO_FORMAT_LINEAR_PCM, AUDIO_QUEUE_BUFFER_BYTES,
    AUDIO_QUEUE_BUFFER_COUNT, BITS_PER_CHANNEL, BYTES_PER_FRAME, CHANNELS, SAMPLE_RATE,
};

/// One AudioQueue on the default output and the context its callback receives.
pub(super) struct OutputQueue {
    queue: *mut c_void,
    context: *mut CallbackContext,
}

// One thread at a time calls into the queue: the supervisor, then the dropping
// thread after joining it. The context box is freed only after a dispose.
unsafe impl Send for OutputQueue {}

impl OutputQueue {
    /// Create a queue on the current default output with silent buffers
    /// enqueued, and start it.
    pub(super) fn open(shared: &Arc<Shared>) -> Result<Self, String> {
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
        let context = Box::into_raw(Box::new(CallbackContext {
            shared: Arc::clone(shared),
        }));
        let mut queue = ptr::null_mut();
        let status = unsafe {
            AudioQueueNewOutput(
                &format,
                Some(output_callback),
                context.cast(),
                ptr::null_mut(),
                ptr::null(),
                0,
                &mut queue,
            )
        };
        if status != 0 {
            unsafe { drop(Box::from_raw(context)) };
            return Err(status_error("AudioQueueNewOutput", status));
        }

        for _ in 0..AUDIO_QUEUE_BUFFER_COUNT {
            let mut buffer = ptr::null_mut();
            let status =
                unsafe { AudioQueueAllocateBuffer(queue, AUDIO_QUEUE_BUFFER_BYTES, &mut buffer) };
            if status != 0 {
                unsafe { dispose_failed_queue(queue, context, AudioQueueDispose) };
                return Err(status_error("AudioQueueAllocateBuffer", status));
            }
            unsafe { fill_with_silence(buffer) };
            let status = unsafe { AudioQueueEnqueueBuffer(queue, buffer, 0, ptr::null()) };
            if status != 0 {
                unsafe { dispose_failed_queue(queue, context, AudioQueueDispose) };
                return Err(status_error("AudioQueueEnqueueBuffer", status));
            }
        }

        let output = Self { queue, context };
        let status = output.start();
        if status != 0 {
            unsafe { dispose_failed_queue(queue, context, AudioQueueDispose) };
            return Err(status_error("AudioQueueStart", status));
        }
        Ok(output)
    }

    fn start(&self) -> i32 {
        unsafe { AudioQueueStart(self.queue, ptr::null()) }
    }

    /// Keeps the enqueued buffers; `start` resumes them.
    fn pause(&self) -> i32 {
        unsafe { AudioQueuePause(self.queue) }
    }

    /// Stop immediately and dispose: the teardown's lifecycle statuses.
    fn close(&mut self) -> [i32; 2] {
        let stop = unsafe { AudioQueueStop(self.queue, 1) };
        let dispose = unsafe { AudioQueueDispose(self.queue, 1) };
        if dispose == 0 {
            unsafe { drop(Box::from_raw(self.context)) };
        }
        (self.queue, self.context) = (ptr::null_mut(), ptr::null_mut());
        [stop, dispose]
    }
}

/// The queue and its supervisor thread.
pub(super) struct Output {
    queue: Arc<Mutex<OutputQueue>>,
    shared: Arc<Shared>,
    supervisor: Option<JoinHandle<()>>,
}

impl Output {
    /// Supervise a started queue. Without the thread the queue just keeps
    /// running, as it did before the idle pause.
    pub(super) fn supervise(queue: OutputQueue, shared: &Arc<Shared>) -> Self {
        let queue = Arc::new(Mutex::new(queue));
        let (owned, thread_shared) = (Arc::clone(&queue), Arc::clone(shared));
        let supervisor = thread::Builder::new()
            .name("hda-coreaudio".into())
            .spawn(move || supervise(&owned, &thread_shared))
            .map_err(|error| eprintln!("hda CoreAudio output: no idle pause, {error}"))
            .ok();
        Self {
            queue,
            shared: Arc::clone(shared),
            supervisor,
        }
    }

    /// End the supervisor, then stop and dispose the queue it left: the
    /// lifecycle statuses the teardown prints.
    pub(super) fn close(&mut self) -> [i32; 2] {
        self.shared.output.request_stop();
        if let Some(supervisor) = self.supervisor.take() {
            supervisor.thread().unpark();
            if supervisor.join().is_err() {
                eprintln!("hda CoreAudio output: supervisor panicked");
            }
        }
        lock(&self.queue).close()
    }
}

fn lock(queue: &Mutex<OutputQueue>) -> MutexGuard<'_, OutputQueue> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Pause the queue while the guest is quiet, start it at the next write.
fn supervise(queue: &Mutex<OutputQueue>, shared: &Shared) {
    let control = &shared.output;
    control.register_supervisor(thread::current());
    let mut gate = IdleGate::new(control.writes(), Instant::now());
    while !control.stopping() {
        let now = Instant::now();
        match gate.step(control.writes(), now) {
            Some(Step::Pause) => {
                // Published before the write count is read again, so a write
                // racing this pause is either read next or wakes us.
                control.set_paused(true);
                // A failed pause only leaves the Mac's output running.
                lock(queue).pause();
                continue;
            }
            Some(Step::Start) => {
                control.set_paused(false);
                shared.continuity.note_restart();
                let status = lock(queue).start();
                if status != 0 {
                    eprintln!(
                        "hda CoreAudio output: {}; sound may stay off until the guest's next sound",
                        status_error("AudioQueueStart", status)
                    );
                }
            }
            None => {}
        }
        match gate.sleep(now) {
            Some(timeout) => thread::park_timeout(timeout),
            None => thread::park(),
        }
    }
}
