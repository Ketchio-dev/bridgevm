//! Failed ownership transfer clears only this attempt's staging directory.

use super::record::Record;
use super::recovery::clear_owned;
use crate::media_lease::MediaLease;
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(in crate::snapshot_pair::managed::restore) struct StageOwner {
    path: PathBuf,
    identity: (u64, u64),
    record: Option<Record>,
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
            record: None,
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
        drop(clear_owned(&self.path, self.identity, self.record.as_ref()));
    }
}

#[path = "managed_pair_staging_publication.rs"]
mod publication;
#[cfg(test)]
#[path = "managed_pair_staging_ownership_tests.rs"]
mod tests;

#[path = "managed_pair_staging_complete.rs"]
mod complete;
