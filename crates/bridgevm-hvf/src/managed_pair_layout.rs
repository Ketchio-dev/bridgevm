use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(super) fn store_root(disk: &Path, vars: &Path) -> PathBuf {
    identity::stable_root(disk, vars)
}

pub(super) fn resolve_root(disk: &Path, vars: &Path) -> io::Result<PathBuf> {
    identity::resolve(disk, vars)
}
#[path = "managed_pair_identity.rs"]
mod identity;

pub(super) fn initialize(root: &Path) -> io::Result<()> {
    if !private_directory(root, false)? {
        private_directory(root, true)?;
        let marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root.join("original"))?;
        marker.sync_all()?;
        File::open(root)?.sync_all()?;
        File::open(root.parent().unwrap())?.sync_all()?;
    }
    Ok(())
}

pub(super) fn original_paths(
    root: &Path,
    disk: &Path,
    vars: &Path,
) -> io::Result<(PathBuf, PathBuf)> {
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

pub(super) fn acknowledge(root: &Path) -> io::Result<()> {
    match fs::remove_file(root.join("original")) {
        Ok(()) => File::open(root)?.sync_all(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn private_directory(path: &Path, create: bool) -> io::Result<bool> {
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
