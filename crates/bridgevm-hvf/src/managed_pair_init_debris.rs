//! Clear what an interrupted setup left under the unpublished root name.

use super::private_directory;
use std::fs;
use std::io;
use std::path::Path;

/// Setup leaves the sibling empty or holding only the empty marker. Anything
/// else is kept, and the error names the entry that blocks setup.
pub(super) fn clear(building: &Path) -> io::Result<()> {
    remove(building).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "kept managed-storage setup entry {}: interrupted setup leaves \
                 only an empty directory or an empty marker there ({error})",
                building.display()
            ),
        )
    })
}

/// remove_dir refuses any content setup never writes.
fn remove(building: &Path) -> io::Result<()> {
    if !private_directory(building, false)? {
        return Ok(());
    }
    let marker = building.join("original");
    if fs::symlink_metadata(&marker).is_ok_and(|m| m.is_file() && m.len() == 0) {
        fs::remove_file(&marker)?;
    }
    fs::remove_dir(building)
}
