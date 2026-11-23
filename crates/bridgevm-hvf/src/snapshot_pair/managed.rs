//! A cooperating reader selects a complete pair while holding its logical lease.
//! Original paths remain unchanged; callers must use paths(), not the originals.
//!
//! The selected generation lives under a private managed root beside the disk,
//! named from the pair's relative identity. Restore stages a verified pair under
//! that root, then publishes its complete generation as `current`.

use super::files::copy_and_sync;
use super::{inspect_snapshot, SnapshotError, SnapshotManifest};
use crate::media_lease::MediaLease;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub mod runtime;
mod seal;
mod staging;

pub struct LockedPair {
    disk: PathBuf,
    vars: PathBuf,
    root: PathBuf,
    _lease: MediaLease,
}

/// Resolve an existing pair only when a nearby vars candidate matches its exact
/// identity. The sidecar hash is not reversible; never guess from one orphan.
pub fn vars_for_disk(disk: &Path) -> io::Result<Option<PathBuf>> {
    let disk = fs::canonicalize(disk)?;
    let parent = disk.parent().unwrap();
    let mut candidates = Vec::new();
    let metadata = parent.parent().unwrap_or(parent).join("metadata");
    // A pair kept in a directory named "metadata" makes both places the same one.
    let directories = if metadata == parent {
        vec![parent.to_path_buf()]
    } else {
        vec![parent.to_path_buf(), metadata]
    };
    for directory in directories {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path != disk && path.is_file() {
                candidates.push(path);
            }
        }
    }
    let mut found = None;
    for vars in candidates {
        if identity_matches(&disk, &vars)? {
            if found.is_some() {
                return Err(io::Error::other("ambiguous managed media binding"));
            }
            found = Some(vars);
        }
    }
    if found.is_none() {
        // Reuse the existing fail-closed orphan check, without creating a pair.
        resolve_root(&disk, &parent.join(".bridgevm-import-no-vars"))?;
    }
    Ok(found)
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
        let root = resolve_root(&disk, &vars)?;
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
            return original_paths(&self.root, &self.disk, &self.vars);
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
        self.restore_using(snapshot, super::publish::publish)
    }

    fn restore_using(
        &mut self,
        snapshot: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        self.restore_copying(snapshot, copy_and_sync, publish)
    }

    /// `copy` stages one file; tests substitute a streaming copy that stops.
    pub(super) fn restore_copying(
        &mut self,
        snapshot: &Path,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        let (snapshot, manifest) = self.admit_restore(snapshot)?;
        initialize(&self.root, |_| {})?;
        let (staged, copied) = self.stage_restore(&snapshot, &manifest, copy)?;
        let vars = &self.vars;
        staged.publish(&self.root.join("current"), |staged, current| {
            // Gone once the staged pair verified, before the disk changes: a crash in between
            // leaves the old disk without its state, which only lets the Mac copy win.
            super::remove_share_state(vars)?;
            publish(staged, current)
        })?;
        self.own_selected()?;
        acknowledge(&self.root)?;
        Ok(copied)
    }

    /// Verify the source before touching any previous restore's staging: a refused restore
    /// keeps that copy, which may be the only intact one. With none, the manifest and member
    /// types and sizes suffice here; staging hashes its copy, the bytes it publishes.
    fn admit_restore(&self, snapshot: &Path) -> Result<(PathBuf, SnapshotManifest), SnapshotError> {
        let snapshot = fs::canonicalize(snapshot)?;
        if snapshot.starts_with(&self.root) {
            return Err(io::Error::other("restore source must be outside managed storage").into());
        }
        let manifest = inspect_snapshot(&snapshot, seal::previous_attempt(&self.root)?)?;
        self.reclaim_staging()?;
        Ok((snapshot, manifest))
    }
}

// Managed storage is created together with its initial-generation marker.
// The root is built under a sibling name and renamed into place, so an
// interrupted setup leaves either no root or a root with its marker.

/// Internal durable boundaries in initialization; production observes none.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitStage {
    DirectoryCreated,
    MarkerSynced,
    RootPublished,
}

/// An existing root is never repaired here: without `current` or the marker
/// it is indistinguishable from lost media and must keep failing closed.
fn initialize(root: &Path, mut observe: impl FnMut(InitStage)) -> io::Result<()> {
    // Before the root check: an older binary or another actor can create the
    // root beside debris, and the pair lease makes any debris stale.
    let building = building_path(root);
    clear_setup_debris(&building)?;
    if private_directory(root, false)? {
        return Ok(());
    }
    private_directory(&building, true)?;
    observe(InitStage::DirectoryCreated);
    let marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(building.join("original"))?;
    marker.sync_all()?;
    observe(InitStage::MarkerSynced);
    File::open(&building)?.sync_all()?;
    fs::rename(&building, root)?;
    observe(InitStage::RootPublished);
    File::open(root.parent().unwrap())?.sync_all()
}

/// Identity resolution refuses unbound ".bridgevm-pair-*" entries, so the
/// unpublished root must not start with that prefix.
fn building_path(root: &Path) -> PathBuf {
    let mut name = OsString::from(".bridgevm-init-");
    name.push(root.file_name().unwrap());
    root.with_file_name(name)
}

/// Setup leaves the sibling empty or holding only the empty marker. Anything
/// else is kept, and the error names the entry that blocks setup.
fn clear_setup_debris(building: &Path) -> io::Result<()> {
    remove_setup_debris(building).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "kept managed-storage setup entry {}: interrupted setup leaves \
                 only an empty directory or an empty marker there ({error})",
                building.display()
            ),
        )
    })
}

/// remove_dir refuses any content setup never writes.
fn remove_setup_debris(building: &Path) -> io::Result<()> {
    if !private_directory(building, false)? {
        return Ok(());
    }
    let marker = building.join("original");
    if fs::symlink_metadata(&marker).is_ok_and(|m| m.is_file() && m.len() == 0) {
        fs::remove_file(&marker)?;
    }
    fs::remove_dir(building)
}

fn original_paths(root: &Path, disk: &Path, vars: &Path) -> io::Result<(PathBuf, PathBuf)> {
    let marker = fs::symlink_metadata(root.join("original"))?;
    if !marker.file_type().is_file()
        || marker.len() != 0
        || marker.uid() != fs::metadata(root)?.uid()
        || marker.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::other("invalid initial-generation marker"));
    }
    Ok((disk.to_path_buf(), vars.to_path_buf()))
}

fn acknowledge(root: &Path) -> io::Result<()> {
    match fs::remove_file(root.join("original")) {
        Ok(()) => File::open(root)?.sync_all(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
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

fn digest(paths: [&Path; 2]) -> String {
    let mut hash = Sha256::new();
    for path in paths {
        let bytes = path.as_os_str().as_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn stable_root(disk: &Path, vars: &Path) -> PathBuf {
    let parent = disk.parent().unwrap();
    let common = parent
        .components()
        .zip(vars.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative_vars = PathBuf::new();
    for _ in parent.components().skip(common) {
        relative_vars.push("..");
    }
    for component in vars.components().skip(common) {
        relative_vars.push(component.as_os_str());
    }
    let id = digest([Path::new(disk.file_name().unwrap()), &relative_vars]);
    parent.join(format!(".bridgevm-pair-v2-{id}"))
}

fn legacy_root(disk: &Path, vars: &Path) -> PathBuf {
    disk.parent()
        .unwrap()
        .join(format!(".bridgevm-pair-{}", digest([disk, vars])))
}

fn identity_matches(disk: &Path, vars: &Path) -> io::Result<bool> {
    Ok(private_directory(&stable_root(disk, vars), false)?
        || private_directory(&legacy_root(disk, vars), false)?)
}

/// Caller owns logical disk/vars leases. Do not infer a binding from one orphan:
/// it could belong to another VM, and guessing would boot the wrong guest state.
fn resolve_root(disk: &Path, vars: &Path) -> io::Result<PathBuf> {
    let stable = stable_root(disk, vars);
    let legacy = legacy_root(disk, vars);
    let has_stable = private_directory(&stable, false)?;
    let has_legacy = private_directory(&legacy, false)?;
    if has_stable && has_legacy {
        return Err(io::Error::other(
            "conflicting legacy and relative media identities",
        ));
    }
    if has_stable {
        return Ok(stable);
    }
    if has_legacy {
        fs::rename(&legacy, &stable)?;
        File::open(disk.parent().unwrap())?.sync_all()?;
        return Ok(stable);
    }
    for entry in fs::read_dir(disk.parent().unwrap())? {
        if entry?
            .file_name()
            .as_bytes()
            .starts_with(b".bridgevm-pair-")
        {
            return Err(io::Error::other(
                "unbound managed media exists; refusing stale-original fallback",
            ));
        }
    }
    Ok(stable)
}

#[cfg(test)]
mod interruption_tests;
#[cfg(test)]
mod tests;
