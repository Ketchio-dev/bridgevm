//! Atomic publication of an exclusively owned staged snapshot directory.

use std::fs;
use std::io;
use std::path::Path;

#[path = "snapshot_publish_exchange.rs"]
mod exchange;
use exchange::exchange;

pub(super) fn publish(staged: &Path, destination: &Path) -> io::Result<()> {
    publish_with(staged, destination, exchange)
}

fn publish_with(
    staged: &Path,
    destination: &Path,
    swap: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    publish_using(
        staged,
        destination,
        swap,
        |a, b| fs::rename(a, b),
        super::sync_dir,
    )
}

fn publish_using(
    staged: &Path,
    destination: &Path,
    swap: impl FnOnce(&Path, &Path) -> io::Result<()>,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    if staged == destination || staged.parent() != destination.parent() {
        return Err(io::Error::other(
            "snapshot publication needs distinct siblings",
        ));
    }
    if !fs::symlink_metadata(staged)?.file_type().is_dir() {
        return Err(io::Error::other("snapshot staging is not a directory"));
    }
    let exchanged = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            if fs::read_dir(destination)?.next().transpose()?.is_none() {
                // One rename replaces only an empty directory. A late entry
                // makes the syscall refuse; never retry with exchange.
                rename(staged, destination)?;
                false
            } else {
                swap(staged, destination)?;
                true
            }
        }
        Ok(_) => return Err(io::Error::other("snapshot destination is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            rename(staged, destination)?;
            false
        }
        Err(error) => return Err(error),
    };
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    sync(parent)?;
    // After exchange, the old complete snapshot occupies the staging name.
    // Keep it if durability was uncertain; deletion failure only leaves debris.
    if exchanged {
        let _ = fs::remove_dir_all(staged);
    }
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_publish_tests.rs"]
mod tests;
