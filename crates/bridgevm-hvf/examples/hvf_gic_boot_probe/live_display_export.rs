use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::VirtPlatform;
use bridgevm_hvf::ramfb::{RamfbConfig, RamfbSnapshot};

pub struct LiveDisplayExporter {
    path: Option<PathBuf>,
    interval: Duration,
    next_due: Instant,
}

impl LiveDisplayExporter {
    pub fn from_env() -> Self {
        let path = std::env::var_os("BRIDGEVM_DISPLAY_EXPORT_PPM")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let interval_ms = std::env::var("BRIDGEVM_DISPLAY_EXPORT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| (100..=60_000).contains(value))
            .unwrap_or(500);
        Self {
            path,
            interval: Duration::from_millis(interval_ms),
            next_due: Instant::now(),
        }
    }

    pub fn due(&self, now: Instant) -> bool {
        self.path.is_some() && now >= self.next_due
    }

    pub fn export_due(&mut self, platform: &VirtPlatform, now: Instant) {
        if !self.due(now) {
            return;
        }
        self.next_due = now + self.interval;
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let Some(scanout) = platform.virtio_gpu_scanout() else {
            return;
        };
        let config = RamfbConfig {
            addr: 1,
            fourcc: scanout.fourcc,
            flags: 0,
            width: scanout.width,
            height: scanout.height,
            stride: scanout.stride,
        };
        let Ok(ppm) = RamfbSnapshot::ppm_bytes_from_xrgb8888(config, scanout.bytes) else {
            return;
        };
        if let Err(error) = replace_file(path, &ppm) {
            eprintln!(
                "live display export failed: path={} error={error}",
                path.display()
            );
        }
    }
}

fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("ppm.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::replace_file;

    #[test]
    fn live_frame_replace_overwrites_one_bounded_artifact() {
        let dir = std::env::temp_dir().join(format!(
            "bridgevm-live-display-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let path = dir.join("display.ppm");
        replace_file(&path, b"first").unwrap();
        replace_file(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!path.with_extension("ppm.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
