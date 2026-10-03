//! Validate the managed parent before removing its private stale staging.

use super::*;

impl LockedPair {
    pub(in crate::snapshot_pair::managed::restore) fn reclaim_staging(&self) -> io::Result<()> {
        // Do not create storage during admission, or follow a replaced root.
        if layout::private_directory(&self.root, false)? {
            super::debris::clear(&self.root.join("staging"))?;
        }
        Ok(())
    }
}
