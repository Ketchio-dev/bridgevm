//! Publishes ramfb frames to the app's BVFB display file
//! (`BRIDGEVM_DISPLAY_EXPORT_FB`) when the machine has no virtio-gpu device,
//! which is the supported 3D-off configuration: Windows then draws into the
//! GOP framebuffer that ramfb describes, and nothing else writes the file.

use std::time::{Duration, Instant};

use crate::VirtPlatform;
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::ramfb::RamfbConfig;

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
    let ms = value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (100..=60_000).contains(value))
        .unwrap_or(500);
    Duration::from_millis(ms)
}

/// One per process, outside the reboot loop: the display file has a single
/// writer whose sequence keeps advancing across in-process guest resets. A
/// recreated process starts a new writer, which truncates the file in place
/// on its first frame exactly as the virtio-gpu sink does.
pub struct RamfbDisplayExporter {
    target: Option<DisplayFramebuffer>,
    interval: Duration,
    next_due: Instant,
    published: Option<(RamfbConfig, Vec<u8>)>,
    scratch: Vec<u8>,
    failure_logged: bool,
}

impl RamfbDisplayExporter {
    pub fn from_env() -> Self {
        Self::new(DisplayFramebuffer::from_env(), display_export_interval())
    }

    fn new(target: Option<DisplayFramebuffer>, interval: Duration) -> Self {
        Self {
            target,
            interval,
            next_due: Instant::now(),
            published: None,
            scratch: Vec::new(),
            failure_logged: false,
        }
    }

    pub fn due(&self, now: Instant) -> bool {
        self.target.is_some() && now >= self.next_due
    }

    pub fn export_due(&mut self, platform: &VirtPlatform, mem: &dyn GuestMemoryMut, now: Instant) {
        let virtio_gpu = has_virtio_gpu(platform);
        self.export_frame_due(virtio_gpu, platform.ramfb_config(), mem, now);
    }

    fn export_frame_due(
        &mut self,
        virtio_gpu: bool,
        config: Option<RamfbConfig>,
        mem: &dyn GuestMemoryMut,
        now: Instant,
    ) {
        if !self.due(now) {
            return;
        }
        self.next_due = now + self.interval;
        if virtio_gpu {
            // The device's own sink owns the file for the whole process; a
            // second writer with its own sequence counter would corrupt it.
            if let Some(target) = self.target.take() {
                println!(
                    "ramfb display export: off, virtio-gpu owns {}",
                    target.path().display()
                );
            }
            return;
        }
        // Inactive, not XRGB8888, stride below the row, or oversized: skip.
        let Some((config, len)) = config
            .and_then(|config| Some((config, config.framebuffer_len().ok()?)))
            .filter(|(_, len)| *len <= MAX_FRAME_BYTES)
        else {
            return;
        };
        self.scratch.resize(len, 0);
        if !mem.read_into(config.addr, &mut self.scratch) {
            return;
        }
        if let Some((published, bytes)) = &self.published {
            if *published == config && *bytes == self.scratch {
                return;
            }
        }
        let Some(target) = self.target.as_mut() else {
            return;
        };
        let (width, height, stride) = (config.width, config.height, config.stride);
        match target.publish(width, height, stride, config.fourcc, &self.scratch) {
            Ok(()) => {
                if self.published.is_none() {
                    println!("ramfb display export: first frame {width}x{height} published");
                }
                let previous = self.published.take().map(|(_, bytes)| bytes);
                let frame = std::mem::replace(&mut self.scratch, previous.unwrap_or_default());
                self.published = Some((config, frame));
            }
            Err(error) if !self.failure_logged => {
                self.failure_logged = true;
                eprintln!(
                    "ramfb display export failed: path={} error={error}",
                    target.path().display()
                );
            }
            Err(_) => {}
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
