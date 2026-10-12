//! Reclaim pre-existing staging only when it provably never completed.
//!
//! `.NAME.staging` is also a name a user may export to, and a completed export
//! there is indistinguishable from an attempt that finished staging and then
//! died before publication. Creation renames manifest.json into staging last,
//! so only its absence proves no snapshot completed there. Staging holding it
//! is kept for the user to move or remove; this attempt's own staging is still
//! cleared by `clear_staging` after a failure.

use super::admission::{present_directory, refusal};
use super::staging_debris::clear_staging;
use super::MANIFEST_NAME;
use std::{fs, io, path::Path};

/// Runs under the parent and member leases, before staging is claimed.
pub(super) fn reclaim_staging(staging: &Path) -> io::Result<()> {
    let refuse = |why: String| refusal("staging directory", staging, why);
    if !present_directory(staging).map_err(refuse)? {
        return Ok(());
    }
    match fs::symlink_metadata(staging.join(MANIFEST_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => clear_staging(staging),
        Ok(_) => Err(refuse(format!(
            "holds a published {MANIFEST_NAME}, so it may be a completed snapshot \
             (such as an earlier export) rather than an unfinished attempt"
        ))),
        Err(error) => Err(refuse(format!("cannot be inspected: {error}"))),
    }
}
