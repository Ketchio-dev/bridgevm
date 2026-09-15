//! Cooperative process-lifetime media ownership, not an access-control boundary.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
#[path = "media_lease_os.rs"]
mod os;
#[path = "media_lease_update.rs"]
mod update;

#[derive(Debug)]
pub struct MediaLease {
    files: BTreeMap<String, File>,
}

impl MediaLease {
    pub fn acquire<'a>(paths: impl IntoIterator<Item = &'a Path>) -> io::Result<Self> {
        let mut lease = Self {
            files: BTreeMap::new(),
        };
        lease.extend(paths)?;
        Ok(lease)
    }
}

impl Drop for MediaLease {
    fn drop(&mut self) {
        // close alone retains the lock while fork/dup descriptions survive.
        for file in self.files.values() {
            let _ = os::lock(file, libc::LOCK_UN);
        }
    }
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
