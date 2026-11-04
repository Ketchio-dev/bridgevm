//! The sink's AudioQueue and the supervisor thread that pauses, starts and
//! replaces it.
//!
//! The producer only counts writes (hda_coreaudio_idle.rs); the callback and
//! the default-output listener only raise flags. Every AudioQueue call runs on
//! the supervisor, or at teardown on the dropping thread once the supervisor
//! has ended, never under the platform lock. On macOS 27.0.1 coreaudiod dropped
//! its idle-sleep assertion about 8 s after a pause, and 3 s after a stop or
//! dispose.
//!
//! A replacement (hda_coreaudio_rebuild.rs) creates the new queue first, then
//! retires the old one and stops it synchronously before the new one starts,
//! so two queues never run callbacks at once. A retired queue's callbacks
//! neither drain the ring nor hand buffers back, so its stop and dispose count
//! no callback errors and print no lifecycle record. The ring and every counter
//! carry over, so the teardown's lifecycle, stats and continuity records
//! describe the last queue exactly as they described the only one.
//!
//! The supervisor prints nothing itself. Its lines are notices the producer
//! prints at its next write, before the final report's footer
//! (OutputControl::notice).

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Release;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::hda_coreaudio_callback::{fill_with_silence, output_callback};
use super::hda_coreaudio_idle::{IdleGate, Step};
use super::hda_coreaudio_rebuild::{replace, Rebuild, Replaceable, Watch};
use super::hda_coreaudio_stats::Shared;
use super::hda_coreaudio_teardown::dispose_failed_queue;
use super::{
    status_error, AudioQueueAllocateBuffer, AudioQueueDispose, AudioQueueEnqueueBuffer,
    AudioQueueGetProperty, AudioQueueNewOutput, AudioQueuePause, AudioQueueStart, AudioQueueStop,
    AudioStreamBasicDescription, CallbackContext, AUDIO_FORMAT_FLAG_IS_PACKED,
    AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER, AUDIO_FORMAT_LINEAR_PCM, AUDIO_QUEUE_BUFFER_BYTES,
    AUDIO_QUEUE_BUFFER_COUNT, BITS_PER_CHANNEL, BYTES_PER_FRAME, CHANNELS, SAMPLE_RATE,
};

const QUEUE_PROPERTY_IS_RUNNING: u32 = u32::from_be_bytes(*b"aqrn");
/// Test-only: report a default-output change this often while the queue plays.
const TEST_REBUILD_ENV: &str = "BRIDGEVM_HDA_COREAUDIO_TEST_REBUILD_MS";

/// One AudioQueue on the default output and the context its callback receives.
pub(super) struct OutputQueue {
    queue: *mut c_void,
    context: *mut CallbackContext,
    shared: Arc<Shared>,
}

// One thread at a time calls into the queue: the supervisor, then the dropping
// thread after joining it. The context box is freed only after a dispose.
unsafe impl Send for OutputQueue {}

impl OutputQueue {
    /// Create and start a queue on the current default output.
    pub(super) fn open(shared: &Arc<Shared>) -> Result<Self, String> {
        let output = Self::create(shared)?;
        let status = output.play();
        if status != 0 {
            unsafe { dispose_failed_queue(output.queue, output.context, AudioQueueDispose) };
            return Err(status_error("AudioQueueStart", status));
        }
        Ok(output)
    }

    /// A queue on the current default output with silent buffers enqueued,
    /// not started.
    fn create(shared: &Arc<Shared>) -> Result<Self, String> {
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
            retired: AtomicBool::new(false),
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
        Ok(Self {
            queue,
            context,
            shared: Arc::clone(shared),
        })
    }

    /// Start or resume. The first callback after it measures no interval.
    fn play(&self) -> i32 {
        self.shared.continuity.note_restart();
        unsafe { AudioQueueStart(self.queue, ptr::null()) }
    }

    /// Keeps the enqueued buffers; `play` resumes them.
    fn pause(&self) -> i32 {
        self.shared.continuity.note_pause();
        unsafe { AudioQueuePause(self.queue) }
    }

    /// A queue whose running state cannot be read is not running.
    fn is_running(&self) -> bool {
        let (mut running, mut size) = (0u32, 4u32);
        let status = unsafe {
            AudioQueueGetProperty(
                self.queue,
                QUEUE_PROPERTY_IS_RUNNING,
                (&mut running as *mut u32).cast(),
                &mut size,
            )
        };
        status == 0 && running != 0
    }

    /// From now on this queue's callbacks do nothing.
    fn mark_retired(&self) {
        unsafe { (*self.context).retired.store(true, Release) };
    }

    /// Stop immediately and dispose: the teardown's lifecycle statuses. A
    /// synchronous stop returns once the queue's callbacks are done.
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

impl Replaceable for OutputQueue {
    fn start(&self) -> i32 {
        self.play()
    }

    /// Its stop and dispose statuses are not the teardown's.
    fn retire(mut self) {
        self.mark_retired();
        let [stop, dispose] = self.close();
        if stop != 0 || dispose != 0 {
            self.shared.output.notice(format!(
                "hda CoreAudio output: the replaced queue closed with stop={stop} dispose={dispose}"
            ));
        }
    }
}

/// The queue, its supervisor thread and the default-output listener.
pub(super) struct Output {
    queue: Arc<Mutex<OutputQueue>>,
    shared: Arc<Shared>,
    supervisor: Option<JoinHandle<()>>,
    listener: Option<DefaultOutputListener>,
}

impl Output {
    /// Supervise a started queue. Without the thread the queue just keeps
    /// running on the device it started on.
    pub(super) fn supervise(queue: OutputQueue, shared: &Arc<Shared>) -> Self {
        let queue = Arc::new(Mutex::new(queue));
        let (owned, thread_shared) = (Arc::clone(&queue), Arc::clone(shared));
        let test_rebuild = std::env::var(TEST_REBUILD_ENV)
            .ok()
            .and_then(|ms| ms.parse().ok())
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis);
        let supervisor = thread::Builder::new()
            .name("hda-coreaudio".into())
            .spawn(move || supervise(&owned, &thread_shared, test_rebuild))
            .map_err(|error| eprintln!("hda CoreAudio output: no idle pause, {error}"))
            .ok();
        let listener = supervisor
            .is_some()
            .then(|| DefaultOutputListener::add(shared))
            .flatten();
        Self {
            queue,
            shared: Arc::clone(shared),
            supervisor,
            listener,
        }
    }

    /// Stop following the default output and end the supervisor.
    pub(super) fn end_supervision(&mut self) {
        if let Some(listener) = self.listener.take() {
            listener.remove();
        }
        self.shared.output.request_stop();
        if let Some(supervisor) = self.supervisor.take() {
            supervisor.thread().unpark();
            // A panic printed its own message when it happened; nothing is
            // printed here, after the report footer.
            let _ = supervisor.join();
        }
    }

    /// Stop and dispose the queue the supervisor left: the lifecycle statuses
    /// the teardown prints.
    pub(super) fn close(&mut self) -> [i32; 2] {
        self.end_supervision();
        lock(&self.queue).close()
    }
}

fn lock(queue: &Mutex<OutputQueue>) -> MutexGuard<'_, OutputQueue> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Pause the queue while the guest is quiet, start it at the next write, and
/// replace it when the default output changes or it fails.
fn supervise(queue: &Mutex<OutputQueue>, shared: &Arc<Shared>, test_rebuild: Option<Duration>) {
    let control = &shared.output;
    control.register_supervisor(thread::current());
    let started = Instant::now();
    let mut gate = IdleGate::new(control.writes(), started);
    let mut watch = Watch::started(started);
    let mut test_due = test_rebuild.map(|every| started + every);
    // A replacement that failed is retried at every later wake.
    let mut retry: Option<Rebuild> = None;
    while !control.stopping() {
        let now = Instant::now();
        let mut signals = control.take_signals();
        let step = gate.step(control.writes(), now);
        match step {
            Some(Step::Pause) => {
                // Published before the write count is read again, so a write
                // racing this pause is either read next or wakes us.
                control.set_paused(true);
                // A failed pause only leaves the Mac's output running.
                lock(queue).pause();
                watch.note_paused();
            }
            Some(Step::Start) => {
                control.set_paused(false);
                signals.start_failed = lock(queue).play() != 0;
                watch.note_started(now);
            }
            None => {}
        }
        let playing = !gate.paused();
        if let (Some(due), Some(every)) = (test_due, test_rebuild) {
            if playing && now >= due {
                signals.default_output_changed = true;
                test_due = Some(now + every);
            }
        }
        let reason = watch
            .rebuild(signals, now, || lock(queue).is_running())
            .or(retry);
        if let Some(reason) = reason {
            match rebuild(queue, shared, playing) {
                Ok(()) => {
                    control.notice(format!(
                        "hda CoreAudio output: {}; sound now plays on the Mac's current default output",
                        reason.label()
                    ));
                    retry = None;
                }
                Err(error) => {
                    if retry.is_none() {
                        control.notice(format!(
                            "hda CoreAudio output: {}, and the current default output did not open ({error}); trying again while the guest plays",
                            reason.label()
                        ));
                    }
                    retry = Some(reason);
                }
            }
            if playing {
                watch.note_started(Instant::now());
            }
        }
        if step == Some(Step::Pause) {
            continue;
        }
        match gate.sleep(now) {
            Some(timeout) => thread::park_timeout(timeout),
            None => thread::park(),
        }
    }
}

/// Replace the queue with a new one on the current default output, keeping
/// the ring (hda_coreaudio_rebuild.rs::replace).
fn rebuild(queue: &Mutex<OutputQueue>, shared: &Arc<Shared>, play: bool) -> Result<(), String> {
    let fresh = OutputQueue::create(shared)?;
    match replace(&mut *lock(queue), fresh, play) {
        0 => Ok(()),
        status => Err(status_error("AudioQueueStart", status)),
    }
}

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const SYSTEM_OBJECT: u32 = 1;
const DEFAULT_OUTPUT: PropertyAddress = PropertyAddress {
    selector: u32::from_be_bytes(*b"dOut"),
    scope: u32::from_be_bytes(*b"glob"),
    element: 0,
};

type PropertyListener = unsafe extern "C" fn(u32, u32, *const PropertyAddress, *mut c_void) -> i32;

/// A kAudioHardwarePropertyDefaultOutputDevice listener. The HAL calls it on
/// its own notification thread; the probe runs no run loop and needs none
/// (checked on macOS 27.0.1).
struct DefaultOutputListener {
    /// A leaked `Weak<Shared>`: a notification already under way when the
    /// listener is removed may still read it, so it is never freed.
    data: *mut c_void,
}

// Only the pointer moves between threads; what it points to is never freed.
unsafe impl Send for DefaultOutputListener {}

impl DefaultOutputListener {
    fn add(shared: &Arc<Shared>) -> Option<Self> {
        let data = Box::into_raw(Box::new(Arc::downgrade(shared))).cast::<c_void>();
        let status = unsafe {
            AudioObjectAddPropertyListener(
                SYSTEM_OBJECT,
                &DEFAULT_OUTPUT,
                default_output_changed,
                data,
            )
        };
        if status != 0 {
            unsafe { drop(Box::from_raw(data.cast::<Weak<Shared>>())) };
            eprintln!(
                "hda CoreAudio output: will not follow default output changes, {}",
                status_error("AudioObjectAddPropertyListener", status)
            );
            return None;
        }
        Some(Self { data })
    }

    fn remove(self) {
        unsafe {
            AudioObjectRemovePropertyListener(
                SYSTEM_OBJECT,
                &DEFAULT_OUTPUT,
                default_output_changed,
                self.data,
            )
        };
    }
}

unsafe extern "C" fn default_output_changed(
    _object: u32,
    _count: u32,
    _addresses: *const PropertyAddress,
    data: *mut c_void,
) -> i32 {
    if let Some(shared) = unsafe { &*data.cast::<Weak<Shared>>() }.upgrade() {
        shared.output.note_default_output_changed();
    }
    0
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectAddPropertyListener(
        object: u32,
        address: *const PropertyAddress,
        listener: PropertyListener,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectRemovePropertyListener(
        object: u32,
        address: *const PropertyAddress,
        listener: PropertyListener,
        data: *mut c_void,
    ) -> i32;
}
