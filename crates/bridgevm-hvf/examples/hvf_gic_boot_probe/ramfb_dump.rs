use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError},
    virtio_gpu::VirtioGpuScanout,
};

#[path = "ramfb_sample.rs"]
mod ramfb_sample;
pub use ramfb_sample::{RamfbSampleEnvError, RamfbSampleSchedule, RamfbShellObservation};

pub fn print_and_dump(config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) {
    print_and_dump_snapshot(FrameSnapshot::read_ramfb(config, mem));
}

pub fn print_and_dump_with_virtio_gpu(
    gpu: Option<OwnedGpuScanout>,
    config: Option<RamfbConfig>,
    mem: &dyn GuestMemoryMut,
) {
    if let Some(gpu) = gpu {
        print_and_dump_snapshot(FrameSnapshot::from_gpu(&gpu));
    } else {
        print_and_dump(config, mem);
    }
}

fn print_and_dump_snapshot(frame: FrameSnapshot) {
    let source = frame.source_label();
    let snapshot = match frame {
        FrameSnapshot::Inactive => {
            println!("ramfb framebuffer: inactive");
            return;
        }
        FrameSnapshot::Unavailable { config, error } => {
            print_error(config, error);
            return;
        }
        FrameSnapshot::Captured(snapshot) => snapshot,
    };
    print_summary(&snapshot);
    if let Some(dir) = dump_dir() {
        match write_artifacts(&dir, source, &snapshot) {
            Ok(paths) => {
                println!("ramfb framebuffer artifact: raw={}", paths.raw.display());
                println!("ramfb framebuffer artifact: ppm={}", paths.ppm.display());
            }
            Err(error) => println!("ramfb framebuffer dump error: {error}"),
        }
    } else {
        println!("ramfb framebuffer dump: disabled");
    }
}

#[allow(dead_code)]
pub fn print_checkpoint(label: &str, config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) {
    match RamfbCheckpoint::new(label, config, mem).emit(dump_dir().as_deref()) {
        Ok(checkpoint) => println!("{}", checkpoint.line),
        Err(error) => println!("ramfb checkpoint dump error: label={label} error={error}"),
    }
}

pub fn print_checkpoint_with_virtio_gpu(
    label: &str,
    gpu: Option<OwnedGpuScanout>,
    config: Option<RamfbConfig>,
    mem: &dyn GuestMemoryMut,
) {
    match DisplayCheckpoint::new(label, gpu, config, mem).emit(dump_dir().as_deref()) {
        Ok(checkpoint) => println!("{}", checkpoint.line),
        Err(error) => println!("ramfb checkpoint dump error: label={label} error={error}"),
    }
}

pub fn print_sample_rejection(error: &RamfbSampleEnvError) {
    println!(
        "ramfb sample schedule rejected: parse_error={} samples=0",
        error.name()
    );
}

fn dump_dir() -> Option<PathBuf> {
    std::env::var_os("BRIDGEVM_RAMFB_DUMP_DIR").map(PathBuf::from)
}

fn print_summary(snapshot: &RamfbSnapshot) {
    let config = snapshot.config;
    let summary = snapshot.summary;
    println!(
        "ramfb framebuffer summary: addr={:#x} fourcc={:#010x} xrgb8888={} {}x{} stride={} bytes={} pixels={} nonzero_bytes={} nonzero_pixels={} zero_pixels={} unique_colors={} first_nonzero_pixel={:?} checksum64={:#018x}",
        config.addr,
        config.fourcc,
        config.is_xrgb8888(),
        config.width,
        config.height,
        config.stride,
        summary.byte_len,
        summary.pixel_count,
        summary.nonzero_bytes,
        summary.nonzero_pixels,
        summary.zero_pixels,
        summary.unique_colors,
        summary.first_nonzero_pixel,
        summary.checksum64
    );
}

fn print_error(config: RamfbConfig, error: RamfbSnapshotError) {
    println!(
        "ramfb framebuffer unavailable: addr={:#x} fourcc={:#010x} {}x{} stride={} error={:?}",
        config.addr, config.fourcc, config.width, config.height, config.stride, error
    );
}

struct ArtifactPaths {
    raw: PathBuf,
    ppm: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedGpuScanout {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    stride: u32,
    fourcc: u32,
}

impl OwnedGpuScanout {
    pub fn from_scanout(scanout: VirtioGpuScanout<'_>) -> Self {
        Self {
            bytes: scanout.bytes.to_vec(),
            width: scanout.width,
            height: scanout.height,
            stride: scanout.stride,
            fourcc: scanout.fourcc,
        }
    }

    fn config(&self) -> RamfbConfig {
        RamfbConfig {
            addr: 1,
            fourcc: self.fourcc,
            flags: 0,
            width: self.width,
            height: self.height,
            stride: self.stride,
        }
    }
}

enum FrameSnapshot {
    Captured(RamfbSnapshot),
    Unavailable {
        config: RamfbConfig,
        error: RamfbSnapshotError,
    },
    Inactive,
}

impl FrameSnapshot {
    fn read_ramfb(config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) -> Self {
        let Some(config) = config else {
            return Self::Inactive;
        };
        match RamfbSnapshot::read_from(mem, config) {
            Ok(snapshot) => Self::Captured(snapshot),
            Err(error) => Self::Unavailable { config, error },
        }
    }

    fn from_gpu(gpu: &OwnedGpuScanout) -> Self {
        let config = gpu.config();
        match RamfbSnapshot::from_xrgb8888_bytes(config, gpu.bytes.clone()) {
            Ok(snapshot) => Self::Captured(snapshot),
            Err(error) => Self::Unavailable { config, error },
        }
    }

    fn source_label(&self) -> &'static str {
        match self {
            Self::Captured(snapshot) if snapshot.config.addr == 1 => "virtio-gpu",
            Self::Captured(_) | Self::Unavailable { .. } | Self::Inactive => "ramfb",
        }
    }
}

struct RamfbCheckpoint<'a> {
    label: &'a str,
    config: Option<RamfbConfig>,
    mem: &'a dyn GuestMemoryMut,
}

struct CheckpointRecord {
    line: String,
    #[cfg(test)]
    paths: Option<ArtifactPaths>,
}

impl<'a> RamfbCheckpoint<'a> {
    const fn new(label: &'a str, config: Option<RamfbConfig>, mem: &'a dyn GuestMemoryMut) -> Self {
        Self { label, config, mem }
    }

    fn emit(&self, dir: Option<&Path>) -> io::Result<CheckpointRecord> {
        let Some(config) = self.config else {
            return Ok(self.record_without_artifacts("inactive", "none"));
        };
        let snapshot = match RamfbSnapshot::read_from(self.mem, config) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Ok(self.record_unavailable(error));
            }
        };
        let checksum = format!("{:#018x}", snapshot.summary.checksum64);
        let Some(dir) = dir else {
            return Ok(self.record_without_artifacts("captured-dump-disabled", &checksum));
        };
        let paths = write_checkpoint_artifacts(dir, "ramfb", self.label, &snapshot)?;
        let line = format!(
            "ramfb checkpoint: label={} state=captured checksum64={} raw={} ppm={}",
            sanitize_checkpoint_label(self.label),
            checksum,
            paths.raw.display(),
            paths.ppm.display()
        );
        Ok(CheckpointRecord {
            line,
            #[cfg(test)]
            paths: Some(paths),
        })
    }

    fn record_without_artifacts(&self, state: &str, checksum: &str) -> CheckpointRecord {
        CheckpointRecord {
            line: format!(
                "ramfb checkpoint: label={} state={} checksum64={} raw=none ppm=none",
                sanitize_checkpoint_label(self.label),
                state,
                checksum
            ),
            #[cfg(test)]
            paths: None,
        }
    }

    fn record_unavailable(&self, error: RamfbSnapshotError) -> CheckpointRecord {
        CheckpointRecord {
            line: format!(
                "ramfb checkpoint: label={} state=unavailable checksum64=none raw=none ppm=none error={}",
                sanitize_checkpoint_label(self.label),
                sanitize_checkpoint_error(error)
            ),
            #[cfg(test)]
            paths: None,
        }
    }
}

struct DisplayCheckpoint<'a> {
    label: &'a str,
    gpu: Option<OwnedGpuScanout>,
    config: Option<RamfbConfig>,
    mem: &'a dyn GuestMemoryMut,
}

impl<'a> DisplayCheckpoint<'a> {
    const fn new(
        label: &'a str,
        gpu: Option<OwnedGpuScanout>,
        config: Option<RamfbConfig>,
        mem: &'a dyn GuestMemoryMut,
    ) -> Self {
        Self {
            label,
            gpu,
            config,
            mem,
        }
    }

    fn emit(&self, dir: Option<&Path>) -> io::Result<CheckpointRecord> {
        if let Some(gpu) = self.gpu.as_ref() {
            return self.emit_frame(FrameSnapshot::from_gpu(gpu), dir);
        }
        RamfbCheckpoint::new(self.label, self.config, self.mem).emit(dir)
    }

    fn emit_frame(&self, frame: FrameSnapshot, dir: Option<&Path>) -> io::Result<CheckpointRecord> {
        let source = frame.source_label();
        let snapshot = match frame {
            FrameSnapshot::Captured(snapshot) => snapshot,
            FrameSnapshot::Unavailable { error, .. } => {
                return Ok(RamfbCheckpoint::new(self.label, self.config, self.mem)
                    .record_unavailable(error));
            }
            FrameSnapshot::Inactive => {
                return Ok(RamfbCheckpoint::new(self.label, self.config, self.mem)
                    .record_without_artifacts("inactive", "none"));
            }
        };
        let checksum = format!("{:#018x}", snapshot.summary.checksum64);
        let Some(dir) = dir else {
            return Ok(RamfbCheckpoint::new(self.label, self.config, self.mem)
                .record_without_artifacts("captured-dump-disabled", &checksum));
        };
        let paths = write_checkpoint_artifacts(dir, source, self.label, &snapshot)?;
        let line = format!(
            "ramfb checkpoint: label={} state=captured checksum64={} raw={} ppm={}",
            sanitize_checkpoint_label(self.label),
            checksum,
            paths.raw.display(),
            paths.ppm.display()
        );
        Ok(CheckpointRecord {
            line,
            #[cfg(test)]
            paths: Some(paths),
        })
    }
}

fn write_artifacts(
    dir: &Path,
    source: &str,
    snapshot: &RamfbSnapshot,
) -> io::Result<ArtifactPaths> {
    fs::create_dir_all(dir)?;
    let stem = format!(
        "{source}-{}x{}-{:x}-{:016x}",
        snapshot.config.width,
        snapshot.config.height,
        snapshot.config.addr,
        snapshot.summary.checksum64
    );
    let raw = dir.join(format!("{stem}.xrgb8888"));
    let ppm = dir.join(format!("{stem}.ppm"));
    fs::write(&raw, &snapshot.bytes)?;
    fs::write(&ppm, snapshot.ppm_bytes().map_err(snapshot_io_error)?)?;
    Ok(ArtifactPaths { raw, ppm })
}

fn write_checkpoint_artifacts(
    dir: &Path,
    source: &str,
    label: &str,
    snapshot: &RamfbSnapshot,
) -> io::Result<ArtifactPaths> {
    fs::create_dir_all(dir)?;
    let label = sanitize_checkpoint_label(label);
    for index in 0u32.. {
        let stem = format!("{source}-checkpoint-{label}-{index:04}");
        let raw = dir.join(format!("{stem}.xrgb8888"));
        let ppm = dir.join(format!("{stem}.ppm"));
        if raw.exists() || ppm.exists() {
            continue;
        }
        write_new_file(&raw, &snapshot.bytes)?;
        write_new_file(&ppm, &snapshot.ppm_bytes().map_err(snapshot_io_error)?)?;
        return Ok(ArtifactPaths { raw, ppm });
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no checkpoint artifact suffix available",
    ))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)
}

fn sanitize_checkpoint_label(label: &str) -> String {
    sanitize_checkpoint_token(label, "checkpoint")
}

fn sanitize_checkpoint_error(error: RamfbSnapshotError) -> String {
    sanitize_checkpoint_token(&format_snapshot_error(error), "error")
}

fn sanitize_checkpoint_token(value: &str, fallback: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' => {
                sanitized.push(char::from(byte));
            }
            _ => sanitized.push('-'),
        }
    }
    if sanitized.is_empty() {
        return String::from(fallback);
    }
    sanitized
}

fn format_snapshot_error(error: RamfbSnapshotError) -> String {
    format!("{error:?}")
}

fn snapshot_io_error(error: RamfbSnapshotError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}"))
}

#[cfg(test)]
#[path = "ramfb_dump_tests.rs"]
mod ramfb_dump_tests;
