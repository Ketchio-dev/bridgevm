//! Publishes ramfb frames to the app's BVFB display file
//! (`BRIDGEVM_DISPLAY_EXPORT_FB`) when the machine has no virtio-gpu device,
//! which is the supported 3D-off configuration: Windows then draws into the
//! GOP framebuffer that ramfb describes, and nothing else writes the file.
//! A dedicated host thread (`ramfb_display_thread.rs`) takes the frames; no
//! vCPU copies them, and the platform lock is never held across the copy.

use std::time::Duration;

use crate::VirtPlatform;
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::ramfb::RamfbConfig;

#[path = "ramfb_display_thread.rs"]
mod thread;
pub use thread::RamfbDisplayThread;

/// Guest-programmed geometry above this is not exported, because the copy
/// buffer is allocated before guest RAM backs the range. 64 MiB is 4096x4096
/// XRGB8888; the firmware's ramfb modes are far smaller.
const MAX_FRAME_BYTES: usize = 64 << 20;

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

/// The display file's single writer, owned by the export thread for the
/// whole process: its sequence keeps advancing across in-process guest
/// resets. A recreated process starts a new writer, which reopens the file in
/// place on its first frame, never shrinking it, as the virtio-gpu sink does.
struct FramePublisher {
    target: DisplayFramebuffer,
    published: Option<(RamfbConfig, Vec<u8>)>,
    scratch: Vec<u8>,
    failure_logged: bool,
}

impl FramePublisher {
    fn new(target: DisplayFramebuffer) -> Self {
        Self {
            target,
            published: None,
            scratch: Vec::new(),
            failure_logged: false,
        }
    }

    /// Publish the guest's current frame if its bytes or geometry changed
    /// since the last published one; true when a frame was published.
    fn publish_changed(&mut self, config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) -> bool {
        // Inactive, not XRGB8888, stride below the row, or oversized: skip.
        let Some((config, len)) = config
            .and_then(|config| Some((config, config.framebuffer_len().ok()?)))
            .filter(|(_, len)| *len <= MAX_FRAME_BYTES)
        else {
            return false;
        };
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
        match self
            .target
            .publish(width, height, stride, config.fourcc, &self.scratch)
        {
            Ok(()) => {
                if self.published.is_none() {
                    println!("ramfb display export: first frame {width}x{height} published");
                }
                let previous = self.published.take().map(|(_, bytes)| bytes);
                let frame = std::mem::replace(&mut self.scratch, previous.unwrap_or_default());
                self.published = Some((config, frame));
                true
            }
            Err(error) => {
                if !self.failure_logged {
                    self.failure_logged = true;
                    eprintln!(
                        "ramfb display export failed: path={} error={error}",
                        self.target.path().display()
                    );
                }
                false
            }
        }
    }
}

/// `virtio_gpu_resolution` is `Some` whenever the device exists, before the
/// guest has set any scanout, which is the ownership boundary that matters.
fn has_virtio_gpu(platform: &VirtPlatform) -> bool {
    platform.virtio_gpu_resolution().is_some()
}

#[cfg(test)]
#[path = "ramfb_display_export_tests.rs"]
mod tests;
