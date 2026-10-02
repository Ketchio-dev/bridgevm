//! Failed ownership transfer clears only this attempt's staging directory.

use super::debris;
use crate::media_lease::MediaLease;
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) struct StageOwner {
    path: PathBuf,
    identity: (u64, u64),
}

impl StageOwner {
    pub(super) fn capture(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() {
            return Err(io::Error::other("restore staging must be a directory"));
        }
        Ok(Self {
            path: path.to_path_buf(),
            identity: (metadata.dev(), metadata.ino()),
        })
    }

    pub(super) fn claim_media(&self, lease: &mut MediaLease) -> io::Result<()> {
        lease
            .extend([
                self.path.join("disk.raw").as_path(),
                self.path.join("vars.fd").as_path(),
            ])
            .inspect_err(|_| self.clear())
    }

    pub(super) fn clear(&self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
        {
            drop(debris::clear(&self.path));
        }
    }
}

#[cfg(test)]
#[path = "managed_pair_staging_ownership_tests.rs"]
mod tests;
