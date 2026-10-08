//! Replacing or reclaiming a directory must also own its media members.
//! A parent key serializes exporters, but readers own the disk/vars keys.

use super::admission::{admit_destination, present_directory, refusal};
use super::staging_debris::clear_staging;
use super::{DISK_NAME, VARS_NAME};
use crate::media_lease::MediaLease;
use std::{fs, io, path::Path};

pub(super) fn prepare_owned(
    lease: &mut MediaLease,
    destination: &Path,
    staged: &Path,
) -> io::Result<()> {
    admit_destination(destination)?;
    own_members(lease, destination)?;
    own_members(lease, staged)?;
    // Re-admit while the parent and member keys remain held. In particular,
    // refusal to own a leased member must precede all staging reclamation.
    admit_destination(destination)?;
    clear_staging(staged)?;
    fs::create_dir(staged)?;
    // Reserve the fresh member path keys before copying either file.
    own_members(lease, staged)
}

#[cfg(test)]
#[path = "snapshot_create_member_ownership_tests.rs"]
mod tests;

fn own_members(lease: &mut MediaLease, directory: &Path) -> io::Result<()> {
    if !present_directory(directory).map_err(|why| refusal("media directory", directory, why))? {
        return Ok(());
    }
    lease
        .extend([
            directory.join(DISK_NAME).as_path(),
            directory.join(VARS_NAME).as_path(),
        ])
        .map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                error
            } else {
                refusal("media directory", directory, error.to_string())
            }
        })
}
