//! Artifact writing validates the full PPM before creating either file.
use super::{capture_bounds, sanitize_checkpoint_label, snapshot_io_error, ArtifactPaths};
use bridgevm_hvf::ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotSummary};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(super) fn write_artifacts(
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

pub(super) fn write_artifacts_from_bytes(
    dir: &Path,
    source: &str,
    config: RamfbConfig,
    summary: RamfbSnapshotSummary,
    bytes: &[u8],
) -> io::Result<ArtifactPaths> {
    let ppm_bytes = capture_bounds::ppm(config, bytes).map_err(snapshot_io_error)?;
    fs::create_dir_all(dir)?;
    let stem = format!(
        "{source}-{}x{}-{:x}-{:016x}",
        config.width, config.height, config.addr, summary.checksum64
    );
    let raw = dir.join(format!("{stem}.xrgb8888"));
    let ppm = dir.join(format!("{stem}.ppm"));
    fs::write(&raw, bytes)?;
    fs::write(&ppm, ppm_bytes)?;
    Ok(ArtifactPaths { raw, ppm })
}

pub(super) fn write_checkpoint_artifacts(
    dir: &Path,
    source: &str,
    label: &str,
    snapshot: &RamfbSnapshot,
) -> io::Result<ArtifactPaths> {
    write_checkpoint_artifacts_from_bytes(dir, source, label, snapshot.config, &snapshot.bytes)
}

pub(super) fn write_checkpoint_artifacts_from_bytes(
    dir: &Path,
    source: &str,
    label: &str,
    config: RamfbConfig,
    bytes: &[u8],
) -> io::Result<ArtifactPaths> {
    let ppm_bytes = capture_bounds::ppm(config, bytes).map_err(snapshot_io_error)?;
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
        write_new_file(&ppm, &ppm_bytes)?;
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
