//! A private directory is not proof that a completed snapshot is disposable.
//! An export from another VM can occupy this name, just like a crashed restore.
//! Only manifest-less copies can be reclaimed by a later attempt; failure of
//! this attempt still uses StageOwner's identity-bound cleanup separately.

use crate::snapshot_pair::managed::private_directory;
use std::{fs, io, path::Path};

pub(super) fn reclaim(staged: &Path) -> io::Result<()> {
    if !private_directory(staged, false)? {
        return Ok(());
    }
    match fs::symlink_metadata(staged.join("manifest.json")) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            super::super::debris::clear(staged)
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "managed restore staging {} holds manifest.json and may be a completed snapshot; \
                 it was left intact: move it outside managed storage or explicitly remove it \
                 before retrying restore",
                staged.display()
            ),
        )),
        Err(error) => Err(error),
    }
}
