//! A cooperating reader selects a complete pair while holding its logical lease.
//! Original paths remain unchanged; callers must use paths(), not the originals.

use super::{copy_and_sync, verify_snapshot, SnapshotError, SnapshotManifest};
use crate::media_lease::MediaLease;
use layout::private_directory;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub struct LockedPair {
    disk: PathBuf,
    vars: PathBuf,
    root: PathBuf,
    _lease: MediaLease,
    _selected: Option<MediaLease>,
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
            _selected: None,
        };
        pair.own_selected()?;
        Ok(pair)
    }

    fn own_selected(&mut self) -> io::Result<()> {
        let (disk, vars) = self.paths()?;
        if self._selected.is_none() && (disk != self.disk || vars != self.vars) {
            self._selected = Some(MediaLease::acquire([disk.as_path(), vars.as_path()])?);
        }
        Ok(())
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

    fn restore_using(
        &mut self,
        snapshot: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        let snapshot = fs::canonicalize(snapshot)?;
        if snapshot.starts_with(&self.root) {
            return Err(io::Error::other("restore source must be outside managed storage").into());
        }
        let manifest = verify_snapshot(&snapshot)?;
        let needed = manifest
            .disk_bytes
            .checked_add(manifest.vars_bytes)
            .ok_or_else(|| io::Error::other("snapshot size overflow"))?;
        if let Some(available) = super::free_space::available_bytes(self.root.parent().unwrap()) {
            if needed > available {
                return Err(SnapshotError::InsufficientSpace { needed, available });
            }
        }
        layout::initialize(&self.root)?;
        let staged = self.root.join("staging");
        if private_directory(&staged, false)? {
            fs::remove_dir_all(&staged)?;
        }
        private_directory(&staged, true)?;
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&snapshot.join(name), &staged.join(name))?;
        }
        let copied = verify_snapshot(&staged)?;
        if copied != manifest {
            return Err(io::Error::other("snapshot changed while staging restore").into());
        }
        File::open(&staged)?.sync_all()?;
        publish(&staged, &self.root.join("current"))?;
        self.own_selected()?;
        layout::acknowledge(&self.root)?;
        Ok(copied)
    }
}

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
