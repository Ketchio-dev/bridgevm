//! Cooperative process-lifetime media ownership, not an access-control boundary.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct MediaLease {
    files: Vec<File>,
}

impl MediaLease {
    pub fn acquire<'a>(paths: impl IntoIterator<Item = &'a Path>) -> io::Result<Self> {
        let mut keys = BTreeSet::new();
        for path in paths {
            keys.extend(resource_keys(path)?);
        }
        let mut lease = Self { files: Vec::new() };
        if keys.is_empty() {
            return Ok(lease);
        }
        // SAFETY: geteuid has no pointer arguments or additional preconditions.
        let uid = unsafe { libc::geteuid() };
        let root = private_root(uid)?;
        for key in keys {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
                .open(root.join(key))?;
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || metadata.uid() != uid
                || metadata.nlink() != 1
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(io::Error::other("unsafe media lease file"));
            }
            // SAFETY: file owns a live descriptor for the duration of flock.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
            lease.files.push(file);
        }
        Ok(lease)
    }
}

fn private_root(uid: u32) -> io::Result<PathBuf> {
    // A fixed namespace lets GUI processes and sanitized queue workers agree.
    let root = PathBuf::from(format!("/tmp/bridgevm-media-leases-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(&root)?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::other("unsafe media lease directory"));
    }
    Ok(root)
}

fn resource_keys(path: &Path) -> io::Result<Vec<String>> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("media path has no name"))?;
            fs::canonicalize(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?
            .join(name)
        }
        Err(error) => return Err(error),
    };
    let mut bytes = b"path:".to_vec();
    bytes.extend_from_slice(canonical.as_os_str().as_bytes());
    let mut keys = vec![digest_key(&bytes)];
    match fs::metadata(&canonical) {
        Ok(metadata) if metadata.is_file() => {
            let identity = format!("inode:{}:{}", metadata.dev(), metadata.ino());
            keys.push(digest_key(identity.as_bytes()));
        }
        Ok(_) => return Err(io::Error::other("media is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(keys)
}

fn digest_key(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "media_lease_tests.rs"]
mod tests;
