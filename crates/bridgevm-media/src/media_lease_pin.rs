//! Explicit source identities that must outlive path-based publication rebinding.
use super::*;

impl MediaLease {
    /// Retain current path/inode identities until this owner drops, including
    /// across replace(). Failure changes neither pins nor existing ownership.
    pub fn pin<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) -> io::Result<()> {
        let mut pins = BTreeSet::new();
        for path in paths {
            pins.extend(resource_keys(path)?);
        }
        let mut keys: BTreeSet<_> = self.files.keys().cloned().collect();
        keys.extend(pins.iter().cloned());
        self.replace_keys(keys)?;
        self.pinned.extend(pins);
        Ok(())
    }
}
