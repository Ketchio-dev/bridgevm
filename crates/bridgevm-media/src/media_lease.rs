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
    pinned: BTreeSet<String>,
}

impl MediaLease {
    pub fn acquire<'a>(paths: impl IntoIterator<Item = &'a Path>) -> io::Result<Self> {
        let mut lease = Self {
            files: BTreeMap::new(),
            pinned: BTreeSet::new(),
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

#[path = "media_lease_keys.rs"]
mod keys;
use keys::resource_keys;
#[path = "media_lease_pin.rs"]
mod pin;
#[cfg(test)]
#[path = "media_lease_tests.rs"]
mod tests;
