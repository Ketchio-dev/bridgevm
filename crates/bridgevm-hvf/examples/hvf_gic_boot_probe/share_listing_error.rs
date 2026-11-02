//! Fixed reasons and row numbers only; never guest names or field contents.

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
