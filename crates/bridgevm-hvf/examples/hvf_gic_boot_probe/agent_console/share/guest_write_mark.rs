//! Setting the com.apple.quarantine mark of a guest write through a live
//! descriptor (decisions of 2026-10-23 and their follow-up): on the unpublished
//! temporary of a guest file, and on a directory created for a guest path
//! before anything is published in it. A mark the share will not hold refuses
//! the write. ENOTSUP (no extended attributes) and EACCES or EPERM (a deny ACL,
//! a flag) refuse it for good (guest_write_refusal.rs); any other errno is
//! retried at the next guest listing.

use super::super::guest_write_refusal::persistent;
use super::policy::quarantine_mark;
use super::seal_host::SealHost;
use bridgevm_hvf::smccc_trng::EntropySource;
use std::fs::File;
use std::io;
use std::time::SystemTime;

/// Set a fresh mark on `target`; return the mark that was set.
pub(super) fn set_mark(
    target: &File,
    entropy: &mut impl EntropySource,
    host: &mut impl SealHost,
) -> io::Result<String> {
    let mark = quarantine_mark(SystemTime::now(), entropy)?;
    let refused = |error: io::Error| match error.raw_os_error() {
        Some(libc::ENOTSUP) => persistent(
            io::ErrorKind::Unsupported,
            "quarantine refused: share filesystem has no extended attributes",
        ),
        Some(libc::EACCES | libc::EPERM) => {
            persistent(error.kind(), format!("quarantine not set: {error}"))
        }
        _ => io::Error::new(error.kind(), format!("quarantine not set: {error}")),
    };
    host.set_mark(target, mark.as_bytes()).map_err(refused)?;
    Ok(mark)
}
