use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError},
};

#[path = "ramfb_sample.rs"]
mod ramfb_sample;
pub use ramfb_sample::{RamfbSampleEnvError, RamfbSampleSchedule, RamfbShellObservation};

pub fn print_and_dump(config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) {
    let Some(config) = config else {
        println!("ramfb framebuffer: inactive");
        return;
    };
    match RamfbSnapshot::read_from(mem, config) {
        Ok(snapshot) => {
            print_summary(&snapshot);
            if let Some(dir) = dump_dir() {
                match write_artifacts(&dir, &snapshot) {
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
        Err(error) => print_error(config, error),
    }
}

pub fn print_checkpoint(label: &str, config: Option<RamfbConfig>, mem: &dyn GuestMemoryMut) {
    match RamfbCheckpoint::new(label, config, mem).emit(dump_dir().as_deref()) {
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
        let paths = write_checkpoint_artifacts(dir, self.label, &snapshot)?;
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

fn write_artifacts(dir: &Path, snapshot: &RamfbSnapshot) -> io::Result<ArtifactPaths> {
    fs::create_dir_all(dir)?;
    let stem = format!(
        "ramfb-{}x{}-{:x}-{:016x}",
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
    label: &str,
    snapshot: &RamfbSnapshot,
) -> io::Result<ArtifactPaths> {
    fs::create_dir_all(dir)?;
    let label = sanitize_checkpoint_label(label);
    for index in 0u32.. {
        let stem = format!("ramfb-checkpoint-{label}-{index:04}");
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
