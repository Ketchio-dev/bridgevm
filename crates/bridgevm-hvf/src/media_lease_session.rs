//! Pipe-owned native lease session for a cooperating product operation.

use crate::snapshot_pair::managed::LockedPair;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;
#[path = "media_lease_session_pending_delete.rs"]
pub mod pending_delete;
#[path = "media_lease_session_protocol.rs"]
mod protocol;

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
    protocol::hold(LockedPair::open(disk, vars), input, output)
}

pub fn command(arguments: &[String]) -> ExitCode {
    protocol::command(arguments, LockedPair::open)
}

#[cfg(test)]
#[path = "media_lease_session_tests.rs"]
mod tests;
