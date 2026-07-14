use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::VirtPlatform;
use bridgevm_hvf::ramfb::{RamfbConfig, RamfbSnapshot};

pub struct LiveDisplayExporter {
    path: Option<PathBuf>,
    interval: Duration,
    next_due: Instant,
    last_frame: Option<FrameIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameIdentity {
    width: u32,
    height: u32,
    stride: u32,
    fourcc: u32,
    checksum: u64,
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
            last_frame: None,
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
        let identity = FrameIdentity::new(
            scanout.width,
            scanout.height,
            scanout.stride,
            scanout.fourcc,
            scanout.bytes,
        );
        if self.last_frame == Some(identity) && path.exists() {
            return;
        }
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
        } else {
            self.last_frame = Some(identity);
        }
    }
}

impl FrameIdentity {
    fn new(width: u32, height: u32, stride: u32, fourcc: u32, bytes: &[u8]) -> Self {
        let mut checksum = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            checksum ^= u64::from(*byte);
            checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self {
            width,
            height,
            stride,
            fourcc,
            checksum,
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
    use super::{replace_file, FrameIdentity};

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

    #[test]
    fn live_frame_identity_changes_with_pixels_or_geometry() {
        let first = FrameIdentity::new(2, 1, 8, 1, &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            first,
            FrameIdentity::new(2, 1, 8, 1, &[0, 1, 2, 3, 4, 5, 6, 7])
        );
        assert_ne!(
            first,
            FrameIdentity::new(2, 1, 8, 1, &[0, 1, 2, 3, 4, 5, 6, 8])
        );
        assert_ne!(
            first,
            FrameIdentity::new(1, 2, 8, 1, &[0, 1, 2, 3, 4, 5, 6, 7])
        );
    }
}

pub struct FramebufferExporter {
    path: PathBuf,
    file: Option<File>,
    capacity: usize,
    seq: u64,
}

impl FramebufferExporter {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            capacity: 0,
            seq: 0,
        }
    }

    pub fn interval_from_env() -> Duration {
        let interval_ms = std::env::var("BRIDGEVM_DISPLAY_EXPORT_FB_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value >= 1)
            .unwrap_or(16);
        Duration::from_millis(interval_ms)
    }

    pub fn write(
        &mut self,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        bytes: &[u8],
    ) {
        let needed = 64 + (height as usize) * (stride as usize);
        if self.file.is_none() || self.capacity < needed {
            self.file = None;
            self.capacity = 0;
            let opened = (|| -> std::io::Result<File> {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&self.path)?;
                file.set_len(needed as u64)?;
                Ok(file)
            })();
            match opened {
                Ok(file) => {
                    self.file = Some(file);
                    self.capacity = needed;
                }
                Err(e) => {
                    eprintln!(
                        "live fb export failed: path={} error={e}",
                        self.path.display()
                    );
                    return;
                }
            }
        }

        self.seq = self.seq.wrapping_add(1);
        let header = framebuffer_header(width, height, stride, fourcc, self.seq);
        let payload_len = bytes.len().min(needed - 64);
        let write_result = (|| -> std::io::Result<()> {
            let file = self.file.as_ref().expect("framebuffer file is open");
            file.write_all_at(&header, 0)?;
            file.write_all_at(&bytes[..payload_len], 64)?;
            Ok(())
        })();
        if let Err(e) = write_result {
            self.seq = self.seq.wrapping_add(1);
            eprintln!(
                "live fb export failed: path={} error={e}",
                self.path.display()
            );
            self.file = None;
            return;
        }

        self.seq = self.seq.wrapping_add(1);
        let header = framebuffer_header(width, height, stride, fourcc, self.seq);
        let write_result = self
            .file
            .as_ref()
            .expect("framebuffer file is open")
            .write_all_at(&header, 0);
        if let Err(e) = write_result {
            eprintln!(
                "live fb export failed: path={} error={e}",
                self.path.display()
            );
            self.file = None;
        }
    }
}

fn framebuffer_header(
    width: u32,
    height: u32,
    stride: u32,
    fourcc: u32,
    seq: u64,
) -> [u8; 64] {
    let mut header = [0u8; 64];
    header[0..4].copy_from_slice(&0x4256_4642u32.to_le_bytes());
    header[4..8].copy_from_slice(&1u32.to_le_bytes());
    header[8..12].copy_from_slice(&width.to_le_bytes());
    header[12..16].copy_from_slice(&height.to_le_bytes());
    header[16..20].copy_from_slice(&stride.to_le_bytes());
    header[20..24].copy_from_slice(&fourcc.to_le_bytes());
    header[24..32].copy_from_slice(&seq.to_le_bytes());
    header
}
