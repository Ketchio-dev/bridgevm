//! The ramfb display exporter's host thread. vCPU0 used to copy and compare
//! the whole frame inside its automation block, under the platform lock, at
//! most every 100 ms; the display was choppy and the guest paid for the copy.
//! This thread paces itself, reads guest RAM through the existing host
//! mapping without any lock (a frame torn by a guest mid-draw is replaced by
//! the next one), and takes the platform lock only to copy the 28-byte ramfb
//! config and whether a virtio-gpu scanout is bound, with `try_lock`, so it
//! never waits behind a vCPU; that copy is a hold the platform lock record
//! charges (platform_lock_holds.rs). A running exporter counts its tick
//! lateness and publish intervals and leaves its `ramfb display timing:`
//! record with the platform (ramfb_display_timing.rs).
//! It exports when the app's display tick asks for a frame, and on its own
//! deadline only when nobody asks (ramfb_display_loop.rs).

use std::sync::{Arc, Mutex, TryLockError};
use std::thread::JoinHandle;
use std::time::Duration;

#[path = "ramfb_display_loop.rs"]
mod pull;
#[path = "ramfb_display_stop.rs"]
mod stop;
#[path = "ramfb_display_timing.rs"]
mod timing;
#[path = "ramfb_display_wake.rs"]
mod wake;
use pull::export_loop;
use timing::SharedTiming;
use wake::ExportWake;

use super::{ramfb_export_interval, FramePublisher};
use crate::platform_lock_holds::try_lock_platform;
use crate::platform_lock_holds::HoldSite;
use crate::{MappedRam, VirtPlatform};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::ramfb::RamfbConfig;

/// Where the thread learns the guest-programmed ramfb geometry.
trait ConfigSource: Send + 'static {
    fn poll(&mut self) -> Option<RamfbConfig>;
    /// Whether the virtio-gpu device had a bound scanout at the last poll.
    fn virtio_scanout_bound(&self) -> bool {
        false
    }
}

/// `VirtPlatform::ramfb_config` and whether a virtio-gpu scanout is bound,
/// polled once per tick. A busy lock (a vCPU mid-exit, the final report)
/// keeps the last values rather than waiting; both change only when the
/// guest programs a display or the platform resets.
struct PlatformConfig {
    platform: Arc<Mutex<VirtPlatform>>,
    last: Option<RamfbConfig>,
    virtio_bound: bool,
}

impl ConfigSource for PlatformConfig {
    fn poll(&mut self) -> Option<RamfbConfig> {
        match try_lock_platform(&self.platform, HoldSite::Display) {
            Ok(platform) => {
                self.last = platform.ramfb_config();
                self.virtio_bound = platform.virtio_gpu_scanout().is_some();
            }
            Err(TryLockError::WouldBlock) => {}
            // A vCPU panicked while holding the platform: stop exporting.
            Err(TryLockError::Poisoned(_)) => self.last = None,
        }
        self.last
    }

    fn virtio_scanout_bound(&self) -> bool {
        self.virtio_bound
    }
}

/// The process's ramfb display writer. Start it once per probe process,
/// outside the reboot loop, after guest RAM is mapped and before any vCPU
/// runs; dropping it stops and joins the thread, so declare it after the RAM
/// backing: every exit path, unwinding included, then joins before unmapping.
pub struct RamfbDisplayThread {
    wake: Arc<ExportWake>,
    handle: Option<JoinHandle<()>>,
    timing: SharedTiming,
}

impl RamfbDisplayThread {
    /// `ram`/`len` are the live guest-RAM mapping at `machine::RAM_BASE`.
    pub fn start(platform: &Arc<Mutex<VirtPlatform>>, ram: *mut u8, len: usize) -> Self {
        let ram = ram as usize;
        // With a virtio-gpu device, publish through the device's own writer:
        // one lock and one sequence for both producers of the file.
        let target = platform.lock().ok().and_then(|platform| {
            platform
                .display_framebuffer()
                .or_else(DisplayFramebuffer::from_env)
        });
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
        if platform.is_poisoned() {
            eprintln!("ramfb display export: off, platform mutex poisoned");
            return Self::inert();
        }
        let Some(target) = target else {
            return Self::inert();
        };
        let source = PlatformConfig {
            platform: Arc::clone(platform),
            last: None,
            virtio_bound: false,
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
        let wake = Arc::new(ExportWake::new());
        let (signal, timing) = (Arc::clone(&wake), SharedTiming::default());
        let counts = Arc::clone(&timing);
        let spawned = std::thread::Builder::new()
            .name("ramfb-display-export".into())
            .spawn(move || {
                let (ms, path) = (interval.as_millis(), target.path().display());
                println!("ramfb display export: thread started interval_ms={ms} path={path}");
                let watch = &crate::ramfb_dirty_watch::FB_WATCH;
                let publisher = FramePublisher::new(target).with_watch(watch);
                export_loop(publisher, source, &memory(), interval, &signal, &counts);
                watch.release();
            });
        match spawned {
            Ok(handle) => Self {
                wake,
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
            wake: Arc::default(),
            handle: None,
            timing: SharedTiming::default(),
        }
    }
}

impl Drop for RamfbDisplayThread {
    fn drop(&mut self) {
        self.wake.stop();
        // A tick is bounded work, so the join waits at most one frame.
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                eprintln!("ramfb display export: thread panicked; display stopped");
            }
        }
    }
}

#[cfg(test)]
#[path = "ramfb_display_thread_tests.rs"]
mod tests;
