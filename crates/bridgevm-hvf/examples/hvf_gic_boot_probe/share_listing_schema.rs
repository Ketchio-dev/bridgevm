//! Validate every admitted row before the parser can replace an earlier snapshot.

use super::error::{ListingLimitError, ListingRowField};

pub(super) fn check(listing: &str) -> Result<(), ListingLimitError> {
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
