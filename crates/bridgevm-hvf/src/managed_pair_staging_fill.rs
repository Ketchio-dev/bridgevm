//! Copy the three verified source members without precreating clone targets.

use std::{io, path::Path};

pub(super) fn copy_files(
    snapshot: &Path,
    staged: &Path,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
) -> io::Result<()> {
    for name in ["disk.raw", "vars.fd", "manifest.json"] {
        copy(&snapshot.join(name), &staged.join(name))?;
    }
    Ok(())
}
