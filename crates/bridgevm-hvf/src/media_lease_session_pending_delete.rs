//! Lease absent final paths for deletion of a caller-confirmed, never-installed VM.
//! This only owns media names; config/staging guards and deletion belong to the caller.

use super::protocol;
use crate::media_lease::MediaLease;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn canonical_leaf(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "media path has no leaf name")
    })?;
    let parent = fs::canonicalize(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    if !fs::metadata(&parent)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "media parent is not a directory",
        ));
    }
    Ok(parent.join(name))
}

fn acquire(disk: &Path, vars: &Path) -> io::Result<MediaLease> {
    let canonical_disk = canonical_leaf(disk)?;
    let canonical_vars = canonical_leaf(vars)?;
    if canonical_disk == canonical_vars {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "disk and vars must be distinct paths",
        ));
    }
    let owner = MediaLease::acquire([canonical_disk.as_path(), canonical_vars.as_path()])?;
    // Check after ownership, without following leaf symlinks (including dangling
    // ones). Never create placeholders or consult/create a managed pair root.
    for path in [disk, vars, &canonical_disk, &canonical_vars] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "pending-delete media must be absent",
                ));
            }
        }
    }
    Ok(owner)
}

/// READY means both absent final paths are leased until release or pipe failure.
pub fn hold(
    disk: &Path,
    vars: &Path,
    input: &mut impl Read,
    output: &mut impl Write,
) -> io::Result<()> {
    protocol::hold(acquire(disk, vars), input, output)
}

pub fn command(arguments: &[String]) -> ExitCode {
    protocol::command(arguments, acquire)
}
