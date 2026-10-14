//! Validate the managed parent before removing its private stale staging.

use super::*;

impl LockedPair {
    pub(in crate::snapshot_pair::managed::restore) fn reclaim_staging(&self) -> io::Result<()> {
        // Do not create storage during admission, or follow a replaced root.
        if recovery::reclaim(&self.root)? {
            admission::reclaim(&self.root.join("staging"))?;
        }
        Ok(())
    }
}
#[path = "managed_pair_staging_admission.rs"]
mod admission;
