//! Powered-off create, restore and create sizing exclude an owned runtime.
//!
//! An owned runtime (`bridgevm-hvf-runtime`'s `prepare`) holds the image
//! writer leases in `media::lock` from start to exit, but the pair lease that
//! `LockedPair` takes is held only while a guest generation runs. Before the
//! first generation and between reboots the writer leases are the only sign
//! that a runtime owns the media. A powered-off operation therefore takes
//! them first, before any destination or media is touched, and keeps them
//! until it finishes: the operation is refused while a runtime is alive, and
//! a runtime cannot be prepared on that media while the operation runs.
//!
//! Only an existing lease file is taken. A runtime creates it before locking
//! and never removes it, so its absence means no runtime holds the image now;
//! creating one would leave a stray file beside every source, including the
//! members of a snapshot, which then could never be replaced. A runtime that
//! starts mid-operation on such never-run media is still refused at its first
//! generation, which cannot open the pair this operation owns.
//!
//! This must stay out of `LockedPair::open`: every guest generation opens the
//! pair while its runtime holds these writer leases.

use super::{managed::LockedPair, SnapshotError, SnapshotManifest};
use crate::media::lock::{lock_path_for, MediaLease, MediaLockError};
use std::path::Path;
use std::{fs, io};

/// Field order is drop order: the pair lease goes before the writer leases,
/// so a runtime admitted afterwards never finds the pair still owned.
pub(super) struct PoweredOffPair {
    pub(super) pair: LockedPair,
    _writers: [Option<MediaLease>; 2],
}

/// `operation` names this process to a runtime refused while it holds media.
pub(super) fn open(disk: &Path, vars: &Path, operation: &str) -> io::Result<PoweredOffPair> {
    let holder = format!("powered-off snapshot {operation}");
    let writers = [writer(disk, &holder)?, writer(vars, &holder)?];
    Ok(PoweredOffPair {
        pair: LockedPair::open(disk, vars)?,
        _writers: writers,
    })
}

/// Restore holds both writer leases until the new generation is selected.
pub(super) fn restore(
    dir: &Path,
    disk: &Path,
    vars: &Path,
) -> Result<SnapshotManifest, SnapshotError> {
    open(disk, vars, "restore")?.pair.restore(dir)
}

fn writer(image: &Path, holder: &str) -> io::Result<Option<MediaLease>> {
    match fs::symlink_metadata(lock_path_for(image)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    MediaLease::acquire(image, holder)
        .map(Some)
        .or_else(refusal)
}

/// A read-only volume cannot hold a runtime's writer lease either (it opens
/// the file for writing), so media read from one, such as an import source,
/// is not refused.
pub(super) fn refusal(error: MediaLockError) -> io::Result<Option<MediaLease>> {
    let (kind, why) = match &error {
        MediaLockError::Io { source, .. } if source.raw_os_error() == Some(libc::EROFS) => {
            return Ok(None)
        }
        MediaLockError::Held { .. } => (
            io::ErrorKind::WouldBlock,
            "media is in use by a running VM or another snapshot operation",
        ),
        MediaLockError::Io { source, .. } => (source.kind(), "media writer lease unavailable"),
    };
    Err(io::Error::new(kind, format!("{why}: {error}")))
}
