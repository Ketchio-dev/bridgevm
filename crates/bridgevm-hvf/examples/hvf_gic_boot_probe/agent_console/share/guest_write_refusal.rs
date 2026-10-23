//! Guest-write refusals that hold until the guest file changes (follow-up of
//! 2026-10-23). The guest write policy refuses a write whose temporary keeps
//! execute bits, or whose quarantine mark the share will not hold: ENOTSUP,
//! EACCES or EPERM (a deny ACL, a flag), or an msdos/exfat mark that does not
//! read back. Fetching the same unchanged guest file again cannot succeed, so
//! the share remembers these refusals (share_write_failures.rs). Any other
//! failure is retried at the next guest listing, as before.

use std::error::Error;
use std::fmt;
use std::io;

/// The reason a write was refused for good.
#[derive(Debug)]
struct Persistent(String);

impl fmt::Display for Persistent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Persistent {}

/// A refusal that holds until the guest file changes, shown as `reason`.
pub(super) fn persistent(kind: io::ErrorKind, reason: impl Into<String>) -> io::Error {
    io::Error::new(kind, Persistent(reason.into()))
}

/// Whether `error` is such a refusal; any other error is retried.
pub(super) fn is_persistent(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<Persistent>())
}
