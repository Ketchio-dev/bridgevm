//! Grow the selected disk of a powered-off pair, the way Parallels enlarges a
//! full Windows disk.
//!
//! The disk is a raw image whose NVMe namespace size is its file length, read
//! when the runtime opens it, so a longer file is a larger disk from the next
//! boot. `ftruncate` extends it sparsely: instant, and no space is used until
//! the guest writes there.
//!
//! After a snapshot restore the runtime boots the managed generation, not the
//! logical original, so the pair is opened like a powered-off create and the
//! selected file is grown. Owning the pair also refuses a running VM. A
//! snapshot keeps its own copy (an APFS clone or a full copy) and its own size,
//! so restoring it later brings back that disk with its own partition table.

use super::runtime_exclusion;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::ExitCode;

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
    let owner = runtime_exclusion::open(disk, vars, "grow")?;
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
pub fn command(arguments: &[String]) -> ExitCode {
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
#[path = "snapshot_pair_grow_tests.rs"]
mod tests;
