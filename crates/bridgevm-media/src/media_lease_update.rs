//! Acquire every additional key before releasing any existing ownership.

use super::*;

impl MediaLease {
    /// Add current path and inode identities without relocking keys we own.
    /// On error, all prior ownership is retained and partial additions release.
    pub fn extend<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) -> io::Result<()> {
        let mut keys: BTreeSet<_> = self.files.keys().cloned().collect();
        for path in paths {
            keys.extend(resource_keys(path)?);
        }
        self.replace_keys(keys)
    }

    /// Rebind ownership without a gap: acquire missing keys, then release old
    /// keys. The caller must serialize changes to the selected media paths.
    /// Failure preserves all existing locks, including duplicated descriptors.
    pub fn replace<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) -> io::Result<()> {
        let mut keys = BTreeSet::new();
        for path in paths {
            keys.extend(resource_keys(path)?);
        }
        self.replace_keys(keys)
    }

    fn replace_keys(&mut self, keys: BTreeSet<String>) -> io::Result<()> {
        let mut added = Self {
            files: BTreeMap::new(),
        };
        let missing: Vec<_> = keys
            .iter()
            .filter(|key| !self.files.contains_key(*key))
            .collect();
        if !missing.is_empty() {
            let uid = os::uid();
            let root = os::private_root(uid)?;
            for key in missing {
                added.files.insert(key.clone(), lock_key(&root, key, uid)?);
            }
        }
        self.files.append(&mut added.files);
        self.files.retain(|key, file| {
            let keep = keys.contains(key);
            if !keep {
                // Explicit unlock also releases ownership held by fork/dup copies.
                let _ = os::lock(file, libc::LOCK_UN);
            }
            keep
        });
        Ok(())
    }
}

fn lock_key(root: &Path, key: &str, uid: u32) -> io::Result<File> {
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
    os::lock(&file, libc::LOCK_EX | libc::LOCK_NB)?;
    Ok(file)
}

#[cfg(test)]
#[path = "media_lease_update_tests.rs"]
mod tests;
