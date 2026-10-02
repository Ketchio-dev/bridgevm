//! Digests of the pair a cooperating reader selects now, for the live gate.
//!
//! After a restore the original disk and vars paths keep their old bytes and the
//! selected generation lives under the managed root (see `managed`), so a check that
//! hashes the original paths reports a mismatch for a correct restore (T17 r79).

use super::managed::LockedPair;
use super::sha256_file_and_size;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
pub struct SelectedDigest {
    pub disk_bytes: u64,
    pub disk_sha256: String,
    pub vars_bytes: u64,
    pub vars_sha256: String,
}

/// Hashes the selected media while holding the pair's lease, so no generation is
/// published between selection and hashing.
pub fn selected_digest(disk: &Path, vars: &Path) -> io::Result<SelectedDigest> {
    let pair = LockedPair::open(disk, vars)?;
    let (disk, vars) = pair.paths()?;
    let (disk_sha256, disk_bytes) = sha256_file_and_size(&disk)?;
    let (vars_sha256, vars_bytes) = sha256_file_and_size(&vars)?;
    Ok(SelectedDigest {
        disk_bytes,
        disk_sha256,
        vars_bytes,
        vars_sha256,
    })
}

/// `snapshot_pair_cli digest <disk> <vars>`: prints the four manifest-style lines.
pub fn command(arguments: &[String]) -> ExitCode {
    let [disk, vars] = arguments else {
        return ExitCode::from(2);
    };
    match selected_digest(Path::new(disk), Path::new(vars)) {
        Ok(d) => match writeln!(
            io::stdout().lock(),
            "disk_bytes {}\ndisk_sha256 {}\nvars_bytes {}\nvars_sha256 {}",
            d.disk_bytes,
            d.disk_sha256,
            d.vars_bytes,
            d.vars_sha256
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("selected digest: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "snapshot_pair_selected_tests.rs"]
mod tests;
