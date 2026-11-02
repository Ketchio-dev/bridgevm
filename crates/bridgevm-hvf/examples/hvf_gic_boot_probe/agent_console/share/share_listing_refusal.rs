//! Listing refusal diagnostics contain fixed reasons and numeric bounds only.

use crate::agent_console::share_sync;

pub(super) fn report_listing_limit(error: share_sync::ListingLimitError) {
    let limit = match error {
        share_sync::ListingLimitError::TooManyBytes { .. } => Some(share_sync::MAX_LISTING_BYTES),
        share_sync::ListingLimitError::TooManyEntries { .. } => {
            Some(share_sync::MAX_LISTING_ENTRIES)
        }
        share_sync::ListingLimitError::InvalidRow { .. } => None,
    };
    if let Some(limit) = limit {
        println!("BVAGENT SHARE ls-error <listing-limit {error:?} limit={limit}>");
    } else {
        println!("BVAGENT SHARE ls-error <listing-invalid {error:?}>");
    }
}
