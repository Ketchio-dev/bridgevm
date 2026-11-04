//! Publishes ramfb frames to the app's BVFB display file
//! (`BRIDGEVM_DISPLAY_EXPORT_FB`). Windows draws into the GOP framebuffer
//! that ramfb describes whenever no display driver presents elsewhere:
//! firmware, bootmgr, BitLocker, logon before a driver, Safe Mode.
//! A dedicated host thread (`ramfb_display_thread.rs`) takes the frames; no
//! vCPU copies them, and the platform lock is never held across the copy.
//!
//! With a virtio-gpu device both write the file through one shared writer
//! (one lock, one sequence). The device's frames win while its scanout is
//! bound and it published within [`VIRTIO_IDLE`]; otherwise changed ramfb
//! frames are published. The write checks that rule again under the file's
//! lock, in one step, so a device frame published while the exporter read the
//! guest's frame is never overwritten; the ramfb change then stays pending
//! until ramfb owns the display again. Losing the scanout publishes nothing by
//! itself: the device's last frame stays until the ramfb frame changes.
//! Windows changes it whenever it draws there (the boot spinner after
//! ExitBootServices, Basic Display after the driver goes), and the stale ramfb
//! frame left when the driver unloads at shutdown is never shown.

use std::time::Duration;

use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::ramfb::RamfbConfig;

use crate::ramfb_dirty_watch::FbWatch;

#[path = "ramfb_display_thread.rs"]
mod thread;
pub use thread::RamfbDisplayThread;

/// Guest-programmed geometry above this is not exported, because the copy
/// buffer is allocated before guest RAM backs the range. 64 MiB is 4096x4096
/// XRGB8888; the firmware's ramfb modes are far smaller.
const MAX_FRAME_BYTES: usize = 64 << 20;

/// A bound virtio-gpu scanout that published within this long owns the
/// display. A bound scanout left idle longer is stale (the firmware's, while
/// bootmgr or BitLocker draw on ramfb), so changed ramfb frames win.
const VIRTIO_IDLE: Duration = Duration::from_secs(1);

/// Cadence shared with the PPM exporter: `BRIDGEVM_DISPLAY_EXPORT_MS`, 100 ms
/// to 60 s, otherwise 500 ms.
pub fn display_export_interval() -> Duration {
    interval_from(std::env::var("BRIDGEVM_DISPLAY_EXPORT_MS").ok().as_deref())
}

fn interval_from(value: Option<&str>) -> Duration {
    bounded_ms(value, 100).unwrap_or(Duration::from_millis(500))
}

/// The ramfb thread's own period, `BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS`, 16 ms to
/// 60 s, so the app can ask for ~30 fps without also speeding up the PPM
/// exporter or the virtio-gpu readback that `BRIDGEVM_DISPLAY_EXPORT_MS`
/// paces. Unset or out of range, the thread keeps that shared cadence.
fn ramfb_export_interval() -> Duration {
    let var = |name| std::env::var(name).ok();
    ramfb_interval_from(
        var("BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS").as_deref(),
        var("BRIDGEVM_DISPLAY_EXPORT_MS").as_deref(),
    )
}

fn ramfb_interval_from(own: Option<&str>, shared: Option<&str>) -> Duration {
    bounded_ms(own, 16).unwrap_or_else(|| interval_from(shared))
}

fn bounded_ms(value: Option<&str>, min_ms: u64) -> Option<Duration> {
    let ms = value?.parse::<u64>().ok()?;
    (min_ms..=60_000)
        .contains(&ms)
        .then(|| Duration::from_millis(ms))
}

/// The export thread's writer of the display file for the whole process (a
/// share of the virtio-gpu device's writer when the device exists): its
/// sequence keeps advancing across in-process guest resets. A recreated
/// process starts a new writer, which reopens the file in place on its first
/// frame, never shrinking it, as the virtio-gpu sink does.
struct FramePublisher {
    target: DisplayFramebuffer,
    published: Option<(RamfbConfig, Vec<u8>)>,
    scratch: Vec<u8>,
    failure_logged: bool,
    /// Guest write tracking; frames are compared every tick without it.
    watch: Option<&'static FbWatch>,
    /// The virtio-gpu device had a bound scanout at the last poll.
    virtio_bound: bool,
}

impl FramePublisher {
    fn new(target: DisplayFramebuffer) -> Self {
        Self {
            target,
            published: None,
            scratch: Vec::new(),
            failure_logged: false,
            watch: None,
            virtio_bound: false,
        }
    }

    /// Whether the virtio-gpu device has a bound scanout, once per tick.
    fn note_virtio_scanout(&mut self, bound: bool) {
        self.virtio_bound = bound;
    }

    /// How long the device's newest frame holds the display against ramfb:
    /// [`VIRTIO_IDLE`] while its scanout is bound, otherwise not at all.
    fn hold(&self) -> Duration {
        if self.virtio_bound {
            VIRTIO_IDLE
        } else {
            Duration::ZERO
        }
    }

    /// The device presents: its scanout is bound and its frame, newer than
    /// ours, is younger than [`VIRTIO_IDLE`]. Only a shortcut that spares the
    /// read; the write decides again under the file's lock.
    fn virtio_presenting(&self) -> bool {
        let age = self.target.other_frame_age();
        age.is_some_and(|age| age < self.hold())
    }

    /// Read guest pixels only after the guest wrote them.
    fn with_watch(mut self, watch: &'static FbWatch) -> Self {
        self.watch = Some(watch);
        self
    }

    /// Publish the guest's current frame if its bytes or geometry changed
    /// since the last published one and the virtio-gpu device is not
    /// presenting; true when a frame was published.
    fn publish_changed(&mut self, config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) -> bool {
        // Before the watch: a guest write seen now stays pending for later.
        if self.virtio_presenting() {
            return false;
        }
        // Inactive, not XRGB8888, stride below the row, or oversized: skip.
        let frame = config
            .and_then(|config| Some((config, config.framebuffer_len().ok()?)))
            .filter(|(_, len)| *len <= MAX_FRAME_BYTES);
        if let Some(watch) = self.watch {
            let range = frame.map(|(config, len)| (config.addr, len as u64));
            let current = matches!((&self.published, frame), (Some((p, _)), Some((c, _))) if *p == c);
            if !watch.may_have_changed(range, mem) && current {
                return false;
            }
        }
        let Some((config, len)) = frame else {
            return false;
        };
        if self.unchanged_in_place(config, len, mem) {
            return false;
        }
        self.scratch.resize(len, 0);
        if !mem.read_into(config.addr, &mut self.scratch) {
            return false;
        }
        if let Some((published, bytes)) = &self.published {
            if *published == config && *bytes == self.scratch {
                return false;
            }
        }
        let (width, height, stride) = (config.width, config.height, config.stride);
        let hold = self.hold();
        match self
            .target
            .publish_yielding(hold, width, height, stride, config.fourcc, &self.scratch)
        {
            Ok(true) => {
                if self.published.is_none() {
                    println!("ramfb display export: first frame {width}x{height} published");
                }
                let previous = self.published.take().map(|(_, bytes)| bytes);
                let frame = std::mem::replace(&mut self.scratch, previous.unwrap_or_default());
                self.published = Some((config, frame));
                return true;
            }
            // The device published while the frame was read: its frame stays.
            Ok(false) => {}
            Err(error) => {
                if !self.failure_logged {
                    self.failure_logged = true;
                    eprintln!(
                        "ramfb display export failed: path={} error={error}",
                        self.target.path().display()
                    );
                }
            }
        }
        // Unpublished, the change stays pending: read it again next tick.
        if let Some(watch) = self.watch {
            watch.mark_dirty();
        }
        false
    }
}

impl FramePublisher {
    /// An idle desktop is the common case: compare the live guest pixels with
    /// the published frame where they lie, so an unchanged frame costs one
    /// compare and no copy. A frame the guest changes mid-compare is caught on
    /// the next tick.
    fn unchanged_in_place(&self, config: RamfbConfig, len: usize, mem: &dyn GuestMemoryMut) -> bool {
        let Some((published, bytes)) = &self.published else {
            return false;
        };
        if *published != config || bytes.len() != len {
            return false;
        }
        let Some(live) = mem.host_ptr(config.addr, len) else {
            return false;
        };
        // SAFETY: `host_ptr` checked that `len` bytes from `live` lie inside the
        // guest RAM mapping, which outlives the call, and `bytes` holds `len`
        // bytes. No Rust reference to guest memory is formed while vCPUs write it.
        unsafe { libc::memcmp(live.cast(), bytes.as_ptr().cast(), len) == 0 }
    }
}

#[cfg(test)]
#[path = "ramfb_display_export_tests.rs"]
mod tests;
