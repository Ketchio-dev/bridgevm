//! A failed publication names staging it had to keep: a kept manifest makes
//! the next create refuse, so the reason belongs in this refusal.

use super::super::staging_debris::clear_staging;
use super::{identity, OwnedStaging};
use std::io;

/// Without an exchange, only an absent destination or an APFS volume can publish.
const UNSUPPORTED_GUIDANCE: &str =
    "; choose an output path that does not exist yet, or one on an APFS volume";

impl OwnedStaging {
    /// Clearing follows debris rules, so content that arrived meanwhile stays.
    /// Returns why this attempt's staging was kept, or None when it is gone.
    pub(in super::super) fn discard(&self) -> Option<String> {
        match identity(&self.path) {
            Ok(found) if found == self.identity => clear_staging(&self.path)
                .err()
                .map(|error| format!("it could not be cleared ({error})")),
            Ok(found) => Some(format!(
                "the path now names another directory (device and inode {found:?}, created as {:?})",
                self.identity
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => Some(format!("it cannot be inspected ({error})")),
        }
    }

    pub(super) fn refusal(&self, error: io::Error) -> io::Error {
        let guidance = match error.kind() {
            io::ErrorKind::Unsupported => UNSUPPORTED_GUIDANCE,
            _ => "",
        };
        match self.discard() {
            None if guidance.is_empty() => error,
            None => io::Error::new(error.kind(), format!("{error}{guidance}")),
            Some(kept) => io::Error::new(
                error.kind(),
                format!(
                    "{error}{guidance}; this attempt's staging {} was kept: {kept}",
                    self.path.display()
                ),
            ),
        }
    }
}
