//! Guest listing parsing and lexical share-name validation.
//!
//! The guest LS/LSR format is `relpath|size|isDir|mtime`. The split is from the
//! right so a pathological file name containing `|` still round-trips; the
//! numeric fields and ISO mtime emitted by the guest agent never contain that
//! separator. Names the host limits refuse are counted (`share_name_refusals`).

use super::{LsEntry, RefusedNames};

#[path = "share_listing_entries.rs"]
mod entries;
#[path = "share_listing_limits.rs"]
mod limits;
pub use limits::{ListingLimitError, MAX_LISTING_BYTES, MAX_LISTING_ENTRIES};

#[cfg(test)]
pub(super) fn parse_ls(listing: &str) -> Vec<LsEntry> {
    let mut entries = Vec::new();
    parse_ls_into(listing, &mut entries).expect("test listing is within bounds");
    entries
}

/// Refuse invalid rows or exceeded bounds before changing `out`.
pub fn parse_ls_into(
    listing: &str,
    out: &mut Vec<LsEntry>,
) -> Result<RefusedNames, ListingLimitError> {
    limits::check(listing)?;
    Ok(entries::parse_into(listing, out))
}
