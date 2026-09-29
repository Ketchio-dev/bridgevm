//! Clear an earlier attempt's staging only when it holds creation's own files.

use super::admission::{listed, present_directory, refusal};
use super::{DISK_NAME, MANIFEST_NAME, VARS_NAME};
use std::fs;
use std::io;
use std::path::Path;

/// write_file_atomically's temporary name for MANIFEST_NAME.
const MANIFEST_TEMPORARY: &str = ".manifest.json.tmp";

pub(super) fn clear_staging(staging: &Path) -> io::Result<()> {
    clear_staging_then(staging, || {})
}

/// Tests act between listing and removal through `after_listing`.
pub(super) fn clear_staging_then(staging: &Path, after_listing: impl FnOnce()) -> io::Result<()> {
    let refuse = |why: String| refusal("staging directory", staging, why);
    if !present_directory(staging).map_err(refuse)? {
        return Ok(());
    }
    let allowed = [DISK_NAME, VARS_NAME, MANIFEST_NAME, MANIFEST_TEMPORARY];
    let names = listed(staging, &allowed, true).map_err(refuse)?;
    after_listing();
    for name in names {
        // On exFAT, unlinking a file also unlinks its "._" companion.
        match fs::remove_file(staging.join(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            removed => removed?,
        }
    }
    // Not remove_dir_all: an entry that arrived after listing is kept.
    fs::remove_dir(staging)
}
