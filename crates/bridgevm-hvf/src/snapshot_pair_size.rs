//! Logical byte lengths of the currently selected pair, without reading content.

use super::{managed::LockedPair, regular_file};
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
pub struct SelectedSize {
    pub disk_bytes: u64,
    pub vars_bytes: u64,
}

/// Read regular-file metadata while owning both the logical and selected media.
/// Restored generations can differ in size from the preserved original files.
/// No content is read or hashed, even for large sparse disks.
pub fn selected_size(disk: &Path, vars: &Path) -> io::Result<SelectedSize> {
    let pair = LockedPair::open(disk, vars)?;
    let (disk, vars) = pair.paths()?;
    let disk = regular_file::open(&disk)?;
    let vars = regular_file::open(&vars)?;
    Ok(SelectedSize {
        disk_bytes: disk.metadata()?.len(),
        vars_bytes: vars.metadata()?.len(),
    })
}

fn write_report(size: &SelectedSize, mut output: impl Write) -> io::Result<()> {
    writeln!(
        output,
        "disk_bytes {}\nvars_bytes {}",
        size.disk_bytes, size.vars_bytes
    )
}

/// `snapshot_pair_cli size <disk> <vars>`: print exactly two byte-count lines.
pub fn command(arguments: &[String]) -> ExitCode {
    let [disk, vars] = arguments else {
        return ExitCode::from(2);
    };
    match selected_size(Path::new(disk), Path::new(vars)) {
        Ok(size) => match write_report(&size, io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("selected size: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "snapshot_pair_size_tests.rs"]
mod tests;
