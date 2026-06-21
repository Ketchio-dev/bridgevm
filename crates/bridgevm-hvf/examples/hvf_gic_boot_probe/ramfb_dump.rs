use std::{
    fs, io,
    path::{Path, PathBuf},
};

use bridgevm_hvf::{
    fwcfg::GuestMemoryMut,
    ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError},
};

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

fn snapshot_io_error(error: RamfbSnapshotError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}"))
}
