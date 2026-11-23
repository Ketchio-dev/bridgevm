//! The pair a cooperating reader selects now: its digests, its sizes, and
//! growing its disk.
//!
//! After a restore the original disk and vars paths keep their old bytes and
//! the selected generation lives under the managed root (see `managed`), so a
//! check that hashes or sizes the original paths reports the wrong pair for a
//! correct restore.

use super::files::{open_regular, sha256_file_and_size};
use super::managed::LockedPair;
use super::powered_off;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::ExitCode;

/// Digests of the selected pair, for the live gate.
#[derive(Debug, PartialEq, Eq)]
pub struct SelectedDigest {
    pub disk_bytes: u64,
    pub disk_sha256: String,
    pub vars_bytes: u64,
    pub vars_sha256: String,
}

/// Hashes the selected media while holding the pair's lease, so no generation is
/// published between selection and hashing.
pub fn selected_digest(disk: &Path, vars: &Path) -> io::Result<SelectedDigest> {
    let pair = LockedPair::open(disk, vars)?;
    let (disk, vars) = pair.paths()?;
    let (disk_sha256, disk_bytes) = sha256_file_and_size(&disk)?;
    let (vars_sha256, vars_bytes) = sha256_file_and_size(&vars)?;
    Ok(SelectedDigest {
        disk_bytes,
        disk_sha256,
        vars_bytes,
        vars_sha256,
    })
}

/// `snapshot_pair_cli digest <disk> <vars>`: prints the four manifest-style lines.
pub fn digest_command(arguments: &[String]) -> ExitCode {
    let [disk, vars] = arguments else {
        return ExitCode::from(2);
    };
    match selected_digest(Path::new(disk), Path::new(vars)) {
        Ok(d) => match writeln!(
            io::stdout().lock(),
            "disk_bytes {}\ndisk_sha256 {}\nvars_bytes {}\nvars_sha256 {}",
            d.disk_bytes,
            d.disk_sha256,
            d.vars_bytes,
            d.vars_sha256
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("selected digest: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Logical byte lengths of the currently selected pair, without reading content.
#[derive(Debug, PartialEq, Eq)]
pub struct SelectedSize {
    pub disk_bytes: u64,
    pub vars_bytes: u64,
}

/// Read file metadata while owning the logical and selected media; refused like
/// the powered-off create it sizes while a runtime holds them. Restored pairs
/// can differ in size from the originals. No content is read or hashed.
pub fn selected_size(disk: &Path, vars: &Path) -> io::Result<SelectedSize> {
    let owner = powered_off(disk, vars, "size")?;
    let (disk, vars) = owner.pair.paths()?;
    let disk = open_regular(&disk)?;
    let vars = open_regular(&vars)?;
    Ok(SelectedSize {
        disk_bytes: disk.metadata()?.len(),
        vars_bytes: vars.metadata()?.len(),
    })
}

fn write_report(size: &SelectedSize, mut output: impl Write) -> io::Result<()> {
    writeln!(
        output,
        "disk_bytes {}\nvars_bytes {}",
        size.disk_bytes, size.vars_bytes
    )
}

/// `snapshot_pair_cli size <disk> <vars>`: print exactly two byte-count lines.
pub fn size_command(arguments: &[String]) -> ExitCode {
    let [disk, vars] = arguments else {
        return ExitCode::from(2);
    };
    match selected_size(Path::new(disk), Path::new(vars)) {
        Ok(size) => match write_report(&size, io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("selected size: {error}");
            ExitCode::FAILURE
        }
    }
}

// Grow the selected disk of a powered-off pair, the way Parallels enlarges a
// full Windows disk.
//
// The disk is a raw image whose NVMe namespace size is its file length, read
// when the runtime opens it, so a longer file is a larger disk from the next
// boot. `ftruncate` extends it sparsely: instant, and no space is used until
// the guest writes there.
//
// After a snapshot restore the runtime boots the managed generation, not the
// logical original, so the pair is opened like a powered-off create and the
// selected file is grown. Owning the pair also refuses a running VM. A
// snapshot keeps its own copy (an APFS clone or a full copy) and its own size,
// so restoring it later brings back that disk with its own partition table.

/// The NVMe LBA size: the runtime refuses an image that is not a whole number of blocks.
const SECTOR_BYTES: u64 = crate::nvme::LBA_SIZE as u64;

/// Extend the selected disk to `new_bytes`; returns its length afterwards.
/// A disk only grows: a smaller size is refused and the same size changes nothing.
pub fn grow_selected_disk(disk: &Path, vars: &Path, new_bytes: u64) -> io::Result<u64> {
    if new_bytes % SECTOR_BYTES != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{new_bytes} bytes is not a whole number of {SECTOR_BYTES}-byte sectors"),
        ));
    }
    let owner = powered_off(disk, vars, "grow")?;
    let (selected, _) = owner.pair.paths()?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&selected)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the selected disk must be a regular file",
        ));
    }
    let current = metadata.len();
    if new_bytes < current {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a disk only grows: {new_bytes} bytes is smaller than its {current} bytes"),
        ));
    }
    if new_bytes > current {
        file.set_len(new_bytes)?;
        // F_FULLFSYNC on macOS: the new length is durable before the app reports it.
        file.sync_all()?;
    }
    Ok(new_bytes)
}

/// Exit status for a pair a runtime or another powered-off operation holds.
pub const IN_USE_EXIT: u8 = 3;

/// `snapshot_pair_cli grow <disk> <vars> <bytes>`: print `disk_bytes <n>`.
pub fn grow_command(arguments: &[String]) -> ExitCode {
    let [disk, vars, bytes] = arguments else {
        return ExitCode::from(2);
    };
    let Ok(bytes) = bytes.parse::<u64>() else {
        eprintln!("grow: the size must be a number of bytes");
        return ExitCode::from(2);
    };
    match grow_selected_disk(Path::new(disk), Path::new(vars), bytes) {
        Ok(length) => match writeln!(io::stdout().lock(), "disk_bytes {length}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("grow: {error}");
            if error.kind() == io::ErrorKind::WouldBlock {
                ExitCode::from(IN_USE_EXIT)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(test)]
mod tests;
