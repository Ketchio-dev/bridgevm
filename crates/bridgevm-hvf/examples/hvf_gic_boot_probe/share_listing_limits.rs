//! Whole-listing bounds applied before any parsed entry or output mutation.

pub const MAX_LISTING_BYTES: usize = 1024 * 1024;
pub const MAX_LISTING_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingLimitError {
    TooManyBytes { bytes: usize },
    TooManyEntries { at_least: usize },
}

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
    Ok(())
}
