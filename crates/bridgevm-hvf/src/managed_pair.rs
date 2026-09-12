//! A cooperating reader selects a complete pair while holding its logical lease.
//! Original paths remain unchanged; callers must use paths(), not the originals.

use super::{copy_and_sync, verify_snapshot, SnapshotError, SnapshotManifest};
use crate::media_lease::MediaLease;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
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
        let mut hash = Sha256::new();
        for path in [&disk, &vars] {
            let bytes = path.as_os_str().as_bytes();
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        let id: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        let root = disk.parent().unwrap().join(format!(".bridgevm-pair-{id}"));
        let pair = Self {
            disk,
            vars,
            root,
            _lease: lease,
        };
        pair.paths()?;
        Ok(pair)
    }

    /// These paths are valid only while this owner remains alive. Writable
    /// current media is not rehashed: normal guest writes change its content.
    pub fn paths(&self) -> io::Result<(PathBuf, PathBuf)> {
        if !private_directory(&self.root, false)? {
            return Ok((self.disk.clone(), self.vars.clone()));
        }
        let current = self.root.join("current");
        if !private_directory(&current, false)? {
            return Ok((self.disk.clone(), self.vars.clone()));
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
        private_directory(&self.root, true)?;
        // Make the store entry durable before its first generation is selected.
        File::open(self.root.parent().unwrap())?.sync_all()?;
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
        Ok(copied)
    }
}

fn private_directory(path: &Path, create: bool) -> io::Result<bool> {
    if create {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if !create && error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    // SAFETY: geteuid has no preconditions or pointer arguments.
    let uid = unsafe { libc::geteuid() };
    if !metadata.file_type().is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::other(
            "managed storage must be a private owned directory",
        ));
    }
    Ok(true)
}

#[cfg(test)]
#[path = "managed_pair_tests.rs"]
mod tests;
