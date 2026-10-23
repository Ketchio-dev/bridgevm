//! Guest listing names refused by the host name limits (`share_name_limits`).
//!
//! Such a name is dropped like a `..` name, so no GET is issued for it, but it
//! is not dropped silently. A listing keeps only how many names were refused,
//! the fixed reason and byte length of the first few and a fingerprint of them
//! all, so no refused name is stored or printed, and an unchanged set of
//! refused names is reported once rather than on every listing poll.

use super::share_name_limits::PATH_MAX_BYTES;
use super::{fnv1a64, from_guest_rel, normalize_rel, ShareSync};

/// Refused names described one by one per listing; the rest are only counted.
pub const REFUSED_SHOWN: usize = 3;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RefusedNames {
    pub count: usize,
    /// The fixed reason and byte length of each of the first `REFUSED_SHOWN`.
    pub shown: Vec<(&'static str, usize)>,
    fingerprint: u64,
}

impl RefusedNames {
    /// The share key for a listed name, or None. A name refused only by the
    /// host limits is noted; a `..` name is refused unnoted, as before.
    pub fn key(&mut self, listed: &str) -> Option<String> {
        let key = from_guest_rel(listed);
        if key.is_none() && !listed.split(['/', '\\']).any(|part| part == "..") {
            if self.shown.len() < REFUSED_SHOWN {
                let path = normalize_rel(listed).len() >= PATH_MAX_BYTES;
                let reason = if path {
                    "path-too-long"
                } else {
                    "component-too-long"
                };
                self.shown.push((reason, listed.len()));
            }
            self.count += 1;
            self.fingerprint =
                (self.fingerprint ^ fnv1a64(listed.as_bytes())).wrapping_mul(0x100000001b3);
        }
        key
    }
}

impl ShareSync {
    /// Whether `refused` differs from the previous listing's refused names,
    /// which it replaces.
    pub fn refused_names_changed(&mut self, refused: &RefusedNames) -> bool {
        let seen = (refused.count, refused.fingerprint);
        std::mem::replace(&mut self.refused_names_seen, seen) != seen
    }
}
