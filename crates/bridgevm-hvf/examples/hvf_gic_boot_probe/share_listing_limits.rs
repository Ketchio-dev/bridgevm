//! Whole-listing bounds applied before any parsed entry or output mutation.
//!
//! A shared Documents folder or a code repository (.git, node_modules) holds 20,000-200,000
//! entries; the first bounds (4096 entries, 1 MiB) paused such a share (share-bigloss-20261111).
//! These still bound what a guest can make the host hold, about 300 MB at the entry bound. The
//! byte bound keeps the whole reply, base64 and its gen, within one agent line
//! (share_listing_reply.rs), so a listing past it is refused for its size, not discarded unread.

pub const MAX_LISTING_BYTES: usize = 24 * 1024 * 1024 - 64 * 1024;
pub const MAX_LISTING_ENTRIES: usize = 262_144;

#[path = "share_listing_error.rs"]
mod error;
#[path = "share_listing_schema.rs"]
mod schema;
pub use error::ListingLimitError;

pub(super) fn check(listing: &str) -> Result<(), ListingLimitError> {
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
    schema::check(listing)
}
