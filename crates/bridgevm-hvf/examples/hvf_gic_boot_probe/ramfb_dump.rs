use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    platform_virt::VirtPlatform,
    ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError, RamfbSnapshotSummary},
    virtio_gpu::VirtioGpuScanout,
};

#[path = "ramfb_sample.rs"]
mod ramfb_sample;
pub use ramfb_sample::{RamfbSampleEnvError, RamfbSampleSchedule, RamfbShellObservation};

pub fn print_and_dump(config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) {
    print_and_dump_snapshot(FrameSnapshot::read_ramfb(config, mem));
}

pub fn print_and_dump_with_virtio_gpu(
    gpu: Option<VirtioGpuScanout<'_>>,
    config: Option<RamfbConfig>,
    mem: &dyn GuestMemoryMut,
) {
    if let Some(gpu) = gpu {
        print_and_dump_gpu(gpu);
    } else {
        print_and_dump(config, mem);
    }
}

pub fn print_checkpoint_for_platform(
    label: &str,
    platform: &VirtPlatform,
    mem: &dyn GuestMemoryMut,
) {
    print_checkpoint_with_virtio_gpu(
        label,
        platform.virtio_gpu_scanout(),
        platform.ramfb_config(),
        mem,
    );
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
    gpu: Option<VirtioGpuScanout<'_>>,
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
    print_summary_parts(snapshot.config, snapshot.summary);
}

fn print_summary_parts(config: RamfbConfig, summary: RamfbSnapshotSummary) {
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

    fn source_label(&self) -> &'static str {
        match self {
            Self::Captured(_) | Self::Unavailable { .. } | Self::Inactive => "ramfb",
        }
    }
}

fn print_and_dump_gpu(gpu: VirtioGpuScanout<'_>) {
    let config = gpu_config(gpu);
    let summary = match RamfbSnapshot::summarize_xrgb8888_bytes(config, gpu.bytes) {
        Ok(summary) => summary,
        Err(error) => {
            print_error(config, error);
            return;
        }
    };
    print_summary_parts(config, summary);
    if let Some(dir) = dump_dir() {
        match write_artifacts_from_bytes(&dir, "virtio-gpu", config, summary, gpu.bytes) {
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

fn gpu_config(gpu: VirtioGpuScanout<'_>) -> RamfbConfig {
    RamfbConfig {
        addr: 1,
        fourcc: gpu.fourcc,
        flags: 0,
        width: gpu.width,
        height: gpu.height,
        stride: gpu.stride,
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
    gpu: Option<VirtioGpuScanout<'a>>,
    config: Option<RamfbConfig>,
    mem: &'a dyn GuestMemoryMut,
}

impl<'a> DisplayCheckpoint<'a> {
    const fn new(
        label: &'a str,
        gpu: Option<VirtioGpuScanout<'a>>,
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
        if let Some(gpu) = self.gpu {
            return self.emit_gpu(gpu, dir);
        }
        RamfbCheckpoint::new(self.label, self.config, self.mem).emit(dir)
    }

    fn emit_gpu(
        &self,
        gpu: VirtioGpuScanout<'_>,
        dir: Option<&Path>,
    ) -> io::Result<CheckpointRecord> {
        let config = gpu_config(gpu);
        let summary = match RamfbSnapshot::summarize_xrgb8888_bytes(config, gpu.bytes) {
            Ok(summary) => summary,
            Err(error) => {
                return Ok(RamfbCheckpoint::new(self.label, self.config, self.mem)
                    .record_unavailable(error));
            }
        };
        let checksum = format!("{:#018x}", summary.checksum64);
        let Some(dir) = dir else {
            return Ok(RamfbCheckpoint::new(self.label, self.config, self.mem)
                .record_without_artifacts("captured-dump-disabled", &checksum));
        };
        let paths = write_checkpoint_artifacts_from_bytes(
            dir,
            "virtio-gpu",
            self.label,
            config,
            gpu.bytes,
        )?;
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
    write_artifacts_from_bytes(
        dir,
        source,
        snapshot.config,
        snapshot.summary,
        &snapshot.bytes,
    )
}

fn write_artifacts_from_bytes(
    dir: &Path,
    source: &str,
    config: RamfbConfig,
    summary: RamfbSnapshotSummary,
    bytes: &[u8],
) -> io::Result<ArtifactPaths> {
    fs::create_dir_all(dir)?;
    let stem = format!(
        "{source}-{}x{}-{:x}-{:016x}",
        config.width, config.height, config.addr, summary.checksum64
    );
    let raw = dir.join(format!("{stem}.xrgb8888"));
    let ppm = dir.join(format!("{stem}.ppm"));
    fs::write(&raw, bytes)?;
    fs::write(
        &ppm,
        RamfbSnapshot::ppm_bytes_from_xrgb8888(config, bytes).map_err(snapshot_io_error)?,
    )?;
    Ok(ArtifactPaths { raw, ppm })
}

fn write_checkpoint_artifacts(
    dir: &Path,
    source: &str,
    label: &str,
    snapshot: &RamfbSnapshot,
) -> io::Result<ArtifactPaths> {
    write_checkpoint_artifacts_from_bytes(dir, source, label, snapshot.config, &snapshot.bytes)
}

fn write_checkpoint_artifacts_from_bytes(
    dir: &Path,
    source: &str,
    label: &str,
    config: RamfbConfig,
    bytes: &[u8],
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
        write_new_file(&raw, bytes)?;
        write_new_file(
            &ppm,
            &RamfbSnapshot::ppm_bytes_from_xrgb8888(config, bytes).map_err(snapshot_io_error)?,
        )?;
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
