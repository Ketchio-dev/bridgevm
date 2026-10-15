//! The staging directory one create attempt made, known by device and inode.
//!
//! A later attempt never reclaims staging that holds a published manifest, so
//! an attempt whose filling or publication fails clears its own staging. It
//! does so only while the path still names the directory it created: a swap
//! or rename that took effect moved that directory, leaving the replaced
//! snapshot or nothing there, and a foreign replacement is not this attempt's.

use super::admission::admit_destination;
use super::staging_debris::clear_staging;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[derive(Debug)]
pub(super) struct OwnedStaging {
    pub(super) path: PathBuf,
    identity: (u64, u64),
}

impl OwnedStaging {
    /// Create staging empty and record its identity before anything else runs.
    pub(super) fn create(path: &Path) -> io::Result<Self> {
        fs::create_dir(path)?;
        let identity = identity(path)?;
        let path = path.to_path_buf();
        Ok(Self { path, identity })
    }

    /// Re-admit after staging, so a destination that changed meanwhile is kept.
    /// `publish` is snapshot_publish::publish outside tests.
    pub(super) fn publish(
        &self,
        destination: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        admit_destination(destination)
            .and_then(|()| publish(&self.path, destination))
            .inspect_err(|_| self.discard())
    }

    /// Clearing follows debris rules, so content that arrived meanwhile stays.
    pub(super) fn discard(&self) {
        if identity(&self.path).ok() == Some(self.identity) {
            let _ = clear_staging(&self.path);
        }
    }
}

fn identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(test)]
#[path = "snapshot_create_publication_failure_tests.rs"]
mod tests;
