//! Pipe-owned native lease session for a cooperating product operation.

use crate::snapshot_pair::managed::LockedPair;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

pub const READY: &[u8] = b"bridgevm-media-lease-v1\n";
const RELEASE: &[u8] = b"release\n";

/// Ownership precedes READY and lasts until release or transport failure.
/// The caller must keep its operation within this session's lifetime.
pub fn hold(
    disk: &Path,
    vars: &Path,
    input: &mut impl Read,
    output: &mut impl Write,
) -> io::Result<()> {
    let _owner = LockedPair::open(disk, vars)?;
    output.write_all(READY)?;
    output.flush()?;
    let mut command = [0u8; 8];
    input.read_exact(&mut command)?;
    if command != RELEASE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid lease release command",
        ));
    }
    Ok(())
}

pub fn command(arguments: &[String]) -> ExitCode {
    if arguments.len() != 2 {
        return ExitCode::from(2);
    }
    match hold(
        Path::new(&arguments[0]),
        Path::new(&arguments[1]),
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("media lease session: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "media_lease_session_tests.rs"]
mod tests;
