//! Own one snapshot parent before clearing a staging directory.

use super::{staging_path, Path};
use crate::media_lease::MediaLease;
use std::fs;
use std::io;
use std::path::PathBuf;

pub(super) fn claim_staging(destination: &Path) -> io::Result<(MediaLease, PathBuf)> {
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::other("snapshot destination needs a parent"))?;
    // A parent-wide key covers case-equivalent names on case-insensitive APFS.
    // MediaLease refuses directories; this sibling is only a cooperative key.
    let key = parent.join(".bridgevm-snapshot-parent-lease");
    match fs::symlink_metadata(&key) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => return Err(io::Error::other("snapshot lease key path is occupied")),
        Err(error) => return Err(error),
    }
    let lease = MediaLease::acquire([key.as_path()])?;
    let staged = staging_path(destination);
    match fs::remove_dir_all(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir(&staged)?;
    Ok((lease, staged))
}
