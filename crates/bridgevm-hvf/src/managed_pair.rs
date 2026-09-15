//! A cooperating reader selects a complete pair while holding its logical lease.
//! Original paths remain unchanged; callers must use paths(), not the originals.

use super::{SnapshotError, SnapshotManifest};
use crate::media_lease::MediaLease;
use layout::private_directory;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub struct LockedPair {
    disk: PathBuf,
    vars: PathBuf,
    root: PathBuf,
    _lease: MediaLease,
}

impl LockedPair {
    pub fn open(disk: &Path, vars: &Path) -> io::Result<Self> {
        let disk = fs::canonicalize(disk)?;
        let vars = fs::canonicalize(vars)?;
        let lease = MediaLease::acquire([disk.as_path(), vars.as_path()])?;
        let dm = fs::metadata(&disk)?;
        let vm = fs::metadata(&vars)?;
        if (dm.dev(), dm.ino()) == (vm.dev(), vm.ino()) {
            return Err(io::Error::other("disk and vars must be distinct files"));
        }
        let root = layout::resolve_root(&disk, &vars)?;
        let mut pair = Self {
            disk,
            vars,
            root,
            _lease: lease,
        };
        pair.own_selected()?;
        Ok(pair)
    }

    fn own_selected(&mut self) -> io::Result<()> {
        let (disk, vars) = self.paths()?;
        self._lease.replace([
            self.disk.as_path(),
            self.vars.as_path(),
            disk.as_path(),
            vars.as_path(),
        ])
    }

    /// These paths are valid only while this owner remains alive. Writable
    /// current media is not rehashed: normal guest writes change its content.
    pub fn paths(&self) -> io::Result<(PathBuf, PathBuf)> {
        if !private_directory(&self.root, false)? {
            return Ok((self.disk.clone(), self.vars.clone()));
        }
        let current = self.root.join("current");
        if !private_directory(&current, false)? {
            return layout::original_paths(&self.root, &self.disk, &self.vars);
        }
        let disk = current.join("disk.raw");
        let vars = current.join("vars.fd");
        for path in [&disk, &vars] {
            if !fs::symlink_metadata(path)?.file_type().is_file() {
                return Err(io::Error::other("selected media must be regular files"));
            }
        }
        Ok((disk, vars))
    }

    /// Publish both restored files together. A publication error may mean
    /// either complete generation is selected; never infer rollback from Err.
    pub fn restore(&mut self, snapshot: &Path) -> Result<SnapshotManifest, SnapshotError> {
        self.restore_using(snapshot, super::snapshot_publish::publish)
    }
}

#[path = "managed_pair_restore.rs"]
mod restore;

#[path = "managed_pair_layout.rs"]
mod layout;
#[path = "managed_pair_runtime.rs"]
pub mod runtime;

#[cfg(test)]
#[path = "managed_pair_api_tests.rs"]
mod api_tests;
#[cfg(test)]
#[path = "managed_pair_tests.rs"]
mod tests;
