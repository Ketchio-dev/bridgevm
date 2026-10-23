//! The ramfb display exporter's host thread. vCPU0 used to copy and compare
//! the whole frame inside its automation block, under the platform lock, at
//! most every 100 ms; the display was choppy and the guest paid for the copy.
//! This thread paces itself, reads guest RAM through the existing host
//! mapping without any lock (a frame torn by a guest mid-draw is replaced by
//! the next one), and takes the platform lock only to copy the 28-byte ramfb
//! config, with `try_lock`, so it never waits behind a vCPU. A running
//! exporter counts its tick lateness and publish intervals and leaves its
//! `ramfb display timing:` record with the platform (ramfb_display_timing.rs).

use std::sync::{Arc, Mutex, TryLockError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[path = "ramfb_display_stop.rs"]
mod stop;
#[path = "ramfb_display_timing.rs"]
mod timing;
use stop::StopSignal;
use timing::RamfbDisplayTiming;
use timing::SharedTiming;

use super::{has_virtio_gpu, ramfb_export_interval, FramePublisher};
use crate::{MappedRam, VirtPlatform};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::ramfb::RamfbConfig;

/// Where the thread learns the guest-programmed ramfb geometry.
trait ConfigSource: Send + 'static {
    fn poll(&mut self) -> Option<RamfbConfig>;
}

/// `VirtPlatform::ramfb_config`, polled once per tick. A busy lock (a vCPU
/// mid-exit, the final report) keeps the last value rather than waiting; the
/// config changes only when firmware programs ramfb or the platform resets.
struct PlatformConfig {
    platform: Arc<Mutex<VirtPlatform>>,
    last: Option<RamfbConfig>,
}

impl ConfigSource for PlatformConfig {
    fn poll(&mut self) -> Option<RamfbConfig> {
        match self.platform.try_lock() {
            Ok(platform) => self.last = platform.ramfb_config(),
            Err(TryLockError::WouldBlock) => {}
            // A vCPU panicked while holding the platform: stop exporting.
            Err(TryLockError::Poisoned(_)) => self.last = None,
        }
        self.last
    }
}

/// The process's ramfb display writer. Start it once per probe process,
/// outside the reboot loop, after guest RAM is mapped and before any vCPU
/// runs; dropping it stops and joins the thread, so declare it after the RAM
/// backing: every exit path, unwinding included, then joins before unmapping.
pub struct RamfbDisplayThread {
    stop: Arc<StopSignal>,
    handle: Option<JoinHandle<()>>,
    timing: SharedTiming,
}

impl RamfbDisplayThread {
    /// `ram`/`len` are the live guest-RAM mapping at `machine::RAM_BASE`.
    pub fn start(platform: &Arc<Mutex<VirtPlatform>>, ram: *mut u8, len: usize) -> Self {
        let ram = ram as usize;
        let target = DisplayFramebuffer::from_env();
        Self::start_with(platform, target, ramfb_export_interval(), move || {
            MappedRam {
                base: machine::RAM_BASE,
                ptr: ram as *mut u8,
                len,
            }
        })
    }

    fn start_with<M: GuestMemoryMut + 'static>(
        platform: &Arc<Mutex<VirtPlatform>>,
        target: Option<DisplayFramebuffer>,
        interval: Duration,
        memory: impl FnOnce() -> M + Send + 'static,
    ) -> Self {
        let Some(target) = target else {
            return Self::inert();
        };
        // Device presence is fixed when the platform is built, so this one
        // check keeps the thread away for the whole process. The device's
        // own sink owns the file: a second writer with its own sequence
        // counter would corrupt it. Unknown ownership fails closed.
        let path = target.path().display();
        match platform.lock().map(|platform| has_virtio_gpu(&platform)) {
            Ok(false) => {}
            Ok(true) => {
                println!("ramfb display export: off, virtio-gpu owns {path}");
                return Self::inert();
            }
            Err(_) => {
                eprintln!("ramfb display export: off, platform mutex poisoned");
                return Self::inert();
            }
        }
        let source = PlatformConfig {
            platform: Arc::clone(platform),
            last: None,
        };
        let thread = Self::spawn(target, interval, source, memory);
        if thread.handle.is_some() {
            timing::install(platform, &thread.timing);
        }
        thread
    }

    fn spawn<M: GuestMemoryMut + 'static>(
        target: DisplayFramebuffer,
        interval: Duration,
        source: impl ConfigSource,
        memory: impl FnOnce() -> M + Send + 'static,
    ) -> Self {
        let stop = Arc::new(StopSignal::default());
        let (signal, timing) = (Arc::clone(&stop), SharedTiming::default());
        let counts = Arc::clone(&timing);
        let spawned = std::thread::Builder::new()
            .name("ramfb-display-export".into())
            .spawn(move || {
                let (ms, path) = (interval.as_millis(), target.path().display());
                println!("ramfb display export: thread started interval_ms={ms} path={path}");
                let publisher = FramePublisher::new(target);
                export_loop(publisher, source, &memory(), interval, &signal, &counts);
            });
        match spawned {
            Ok(handle) => Self {
                stop,
                handle: Some(handle),
                timing,
            },
            Err(error) => {
                eprintln!("ramfb display export: thread not started: {error}");
                Self::inert()
            }
        }
    }

    fn inert() -> Self {
        Self {
            stop: Arc::default(),
            handle: None,
            timing: SharedTiming::default(),
        }
    }
}

impl Drop for RamfbDisplayThread {
    fn drop(&mut self) {
        self.stop.stop();
        // A tick is bounded work, so the join waits at most one frame.
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                eprintln!("ramfb display export: thread panicked; display stopped");
            }
        }
    }
}

/// Deadline-paced: a tick that overruns resynchronises instead of bursting.
fn export_loop(
    mut publisher: FramePublisher,
    mut source: impl ConfigSource,
    mem: &dyn GuestMemoryMut,
    interval: Duration,
    stop: &StopSignal,
    counts: &Mutex<RamfbDisplayTiming>,
) {
    let started = Instant::now();
    let (mut next, mut polls, mut published) = (started, 0u64, 0u64);
    let mut slowest = Duration::ZERO;
    while !stop.wait_until(next) {
        let tick = Instant::now();
        let fresh = publisher.publish_changed(source.poll(), mem);
        published += u64::from(fresh);
        polls += 1;
        let done = Instant::now();
        timing::lock(counts).note_tick(next, tick, fresh.then_some(done));
        slowest = slowest.max(done - tick);
        next = (next + interval).max(done);
    }
    let seconds = started.elapsed().as_secs_f64().max(f64::EPSILON);
    let (poll_hz, publish_hz) = (polls as f64 / seconds, published as f64 / seconds);
    println!(
        "ramfb display export: stopped polls={polls} published={published} \
         seconds={seconds:.1} poll_hz={poll_hz:.1} publish_hz={publish_hz:.1} \
         slowest_tick_us={}",
        slowest.as_micros()
    );
}

#[cfg(test)]
#[path = "ramfb_display_thread_tests.rs"]
mod tests;
