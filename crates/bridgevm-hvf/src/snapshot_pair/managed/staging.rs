//! Finish and own the staged inodes before they can become current media.
//!
//! A private staging name does not make unrelated contents disposable: only
//! known restore copies are cleared, and unfamiliar content is kept as
//! recovery evidence.

use super::seal::{self, Record};
use super::{private_directory, LockedPair};
use crate::media_lease::MediaLease;
use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

impl LockedPair {
    pub(super) fn stage_restore(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    ) -> Result<(StageOwner, SnapshotManifest), SnapshotError> {
        let staged = self.root.join("staging");
        self.reclaim_staging()?;
        private_directory(&staged, true)?;
        let mut owned = StageOwner::capture(&staged)?;
        let copied = owned.fill(snapshot, manifest, copy)?;
        owned.claim_media(&mut self._lease)?;
        Ok((owned, copied))
    }

    /// Validate the managed parent before removing its private stale staging.
    pub(super) fn reclaim_staging(&mut self) -> io::Result<()> {
        // Do not create storage during admission, or follow a replaced root.
        if seal::reclaim_sealed(&self.root, &mut self._lease)? {
            reclaim_unsealed(&self.root.join("staging"))?;
        }
        Ok(())
    }
}

/// One restore attempt's staging directory, known by device and inode. A
/// failed ownership transfer clears only this attempt's staging directory.
pub(super) struct StageOwner {
    path: PathBuf,
    identity: (u64, u64),
    record: Option<Record>,
}

impl StageOwner {
    fn capture(path: &Path) -> io::Result<Self> {
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

    fn claim_media(&self, lease: &mut MediaLease) -> io::Result<()> {
        lease
            .extend([
                self.path.join("disk.raw").as_path(),
                self.path.join("vars.fd").as_path(),
            ])
            .inspect_err(|_| self.clear())
    }

    fn clear(&self) {
        drop(seal::clear_owned(
            &self.path,
            self.identity,
            self.record.as_ref(),
        ));
    }

    /// Seal a completed attempt durably before its verification read can be observed.
    fn fill(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        let result = (|| {
            copy_files(snapshot, &self.path, copy)?;
            self.record = seal::create(
                self.path.parent().unwrap(),
                &self.path,
                self.identity,
                manifest,
            )?;
            let copied = verify_snapshot(&self.path)?;
            if &copied != manifest {
                return Err(io::Error::other("snapshot changed while staging restore").into());
            }
            File::open(&self.path)?.sync_all()?;
            Ok(copied)
        })();
        result.inspect_err(|_| self.clear())
    }

    /// Retain this attempt's directory identity across publication and uncertainty.
    /// Before publication, failure can clear that exact directory. After a rename
    /// or exchange, it must keep any previous generation now at the staging name.
    pub(super) fn publish(
        &self,
        destination: &Path,
        lease: &mut MediaLease,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        publish(&self.path, destination).inspect_err(|_| self.clear())?;
        if self.record.is_some() {
            seal::retire_current(self.path.parent().unwrap(), lease)?;
        }
        Ok(())
    }
}

/// Copy the three admitted source members without precreating clone targets.
fn copy_files(
    snapshot: &Path,
    staged: &Path,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
) -> io::Result<()> {
    for name in ["disk.raw", "vars.fd", "manifest.json"] {
        copy(&snapshot.join(name), &staged.join(name))?;
    }
    Ok(())
}

/// Clear only known restore copies; unfamiliar content is recovery evidence.
/// Runs before each restore attempt and copy, and after a failed one.
/// A crashed copy must not consume the space needed for its own retry.
pub(super) fn clear_debris(staged: &Path) -> io::Result<()> {
    if private_directory(staged, false)? {
        clear_contents(staged)?;
    }
    Ok(())
}

fn clear_contents(staged: &Path) -> io::Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(staged)? {
        let entry = entry?;
        let name = entry.file_name();
        if !["disk.raw", "vars.fd", "manifest.json"]
            .iter()
            .any(|allowed| name == *allowed)
            || !entry.file_type()?.is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "managed restore staging {} contains an unrecognized entry {name:?}; \
                 it was left intact: move it outside managed storage or explicitly remove it \
                 before retrying restore",
                    staged.display()
                ),
            ));
        }
        names.push(name);
    }
    for name in names {
        fs::remove_file(staged.join(name))?;
    }
    // Preserve anything that appeared after inspection instead of recursing.
    fs::remove_dir(staged)
}

/// A private directory is not proof that a completed snapshot is disposable.
/// An export from another VM can occupy this name, just like a crashed restore.
/// Only manifest-less copies can be reclaimed by a later attempt; failure of
/// this attempt still uses StageOwner's identity-bound cleanup separately.
fn reclaim_unsealed(staged: &Path) -> io::Result<()> {
    if !private_directory(staged, false)? {
        return Ok(());
    }
    match fs::symlink_metadata(staged.join("manifest.json")) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => clear_debris(staged),
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

#[cfg(test)]
mod tests;
