//! A private staging name does not make unrelated contents disposable.

use std::{fs, io, path::Path};

pub(super) fn clear(staged: &Path) -> io::Result<()> {
    let mut names = Vec::new();
    for entry in fs::read_dir(staged)? {
        let entry = entry?;
        let name = entry.file_name();
        if !["disk.raw", "vars.fd", "manifest.json"]
            .iter()
            .any(|allowed| name == *allowed)
            || !entry.file_type()?.is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "managed restore staging {} contains an unrecognized entry {name:?}; \
                 it was left intact: move it outside managed storage or explicitly remove it \
                 before retrying restore",
                    staged.display()
                ),
            ));
        }
        names.push(name);
    }
    for name in names {
        fs::remove_file(staged.join(name))?;
    }
    // Preserve anything that appeared after inspection instead of recursing.
    fs::remove_dir(staged)
}
#[cfg(test)]
#[path = "managed_pair_staging_contents_tests.rs"]
mod tests;
