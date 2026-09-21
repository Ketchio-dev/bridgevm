//! Resolve a snapshot destination without permitting source overlap.

use super::{staging_path, Path, PathBuf};
use std::{fs, io};

pub(super) fn prepare_destination(dest: &Path, sources: [&Path; 4]) -> io::Result<PathBuf> {
    let name = dest
        .file_name()
        .ok_or_else(|| io::Error::other("snapshot destination needs a directory name"))?;
    let parent = dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    // Resolve parent aliases while retaining the final entry: publication must
    // still refuse a destination symlink rather than replace its target.
    let destination = fs::canonicalize(parent)?.join(name);
    for output in [&destination, &staging_path(&destination)] {
        let resolved = match fs::canonicalize(output) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => output.to_path_buf(),
            Err(error) => return Err(error),
        };
        if sources.iter().any(|source| source.starts_with(&resolved)) {
            return Err(io::Error::other("snapshot output overlaps source media"));
        }
    }
    Ok(destination)
}
