//! Guest listing parsing and lexical share-name validation.
//!
//! The guest LS/LSR format is `relpath|size|isDir|mtime`. The split is from the
//! right so a pathological file name containing `|` still round-trips; the
//! numeric fields and ISO mtime emitted by the guest agent never contain that
//! separator. Names the host limits refuse are counted (`share_name_refusals`).
//!
//! A shared Documents folder or a code repository (.git, node_modules) holds 20,000-200,000
//! entries, which the listing bounds admit. They still bound what a guest can make the host
//! hold, about 300 MB at the entry bound. The byte bound keeps the whole reply, base64 and its
//! gen, within one agent line (share_listing_reply.rs), so a listing past it is refused for its
//! size, not discarded unread.

use super::{LsEntry, RefusedNames};

pub const MAX_LISTING_BYTES: usize = 24 * 1024 * 1024 - 64 * 1024;
pub const MAX_LISTING_ENTRIES: usize = 262_144;

// Listing errors carry fixed reasons and row numbers only; never guest names
// or field contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingRowField {
    Fields,
    Size,
    DirectoryFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingLimitError {
    TooManyBytes { bytes: usize },
    TooManyEntries { at_least: usize },
    InvalidRow { row: usize, field: ListingRowField },
}

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
    check_limits(listing)?;
    Ok(parse_into(listing, out))
}

/// Whole-listing bounds, applied before any parsed entry or output mutation.
fn check_limits(listing: &str) -> Result<(), ListingLimitError> {
    if listing.len() > MAX_LISTING_BYTES {
        return Err(ListingLimitError::TooManyBytes {
            bytes: listing.len(),
        });
    }
    // Count raw nonempty lines, including malformed, duplicate and refused names.
    if listing
        .lines()
        .filter(|line| !line.is_empty())
        .nth(MAX_LISTING_ENTRIES)
        .is_some()
    {
        return Err(ListingLimitError::TooManyEntries {
            at_least: MAX_LISTING_ENTRIES + 1,
        });
    }
    check_schema(listing)
}

/// Validate every admitted row before the parser can replace an earlier snapshot.
fn check_schema(listing: &str) -> Result<(), ListingLimitError> {
    for (index, line) in listing.lines().filter(|line| !line.is_empty()).enumerate() {
        let row = index + 1;
        let invalid = |field| ListingLimitError::InvalidRow { row, field };
        let mut parts = line.rsplitn(4, '|');
        let fields = (parts.next(), parts.next(), parts.next(), parts.next());
        let (Some(_mtime), Some(directory), Some(size), Some(_name)) = fields else {
            return Err(invalid(ListingRowField::Fields));
        };
        size.parse::<u64>()
            .map_err(|_| invalid(ListingRowField::Size))?;
        if !matches!(directory, "0" | "1") {
            return Err(invalid(ListingRowField::DirectoryFlag));
        }
    }
    Ok(())
}

/// Parse an admitted guest listing with the name and field rules.
fn parse_into(listing: &str, out: &mut Vec<LsEntry>) -> RefusedNames {
    out.clear();
    let mut refused = RefusedNames::default();
    for line in listing.lines().filter(|line| !line.is_empty()) {
        let mut parts = line.rsplitn(4, '|');
        let Some(mtime) = parts.next() else {
            continue;
        };
        let Some(is_dir) = parts.next() else {
            continue;
        };
        let Some(size) = parts.next().and_then(|part| part.parse().ok()) else {
            continue;
        };
        let Some(name) = parts.next().and_then(|listed| refused.key(listed)) else {
            continue;
        };
        out.push(LsEntry {
            name,
            size,
            is_dir: is_dir == "1",
            mtime: mtime.to_string(),
        });
    }
    refused
}
