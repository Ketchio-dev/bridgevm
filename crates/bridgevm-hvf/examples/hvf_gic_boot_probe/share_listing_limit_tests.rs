//! Listing admission boundaries preserve the previous output on refusal.

use super::*;

#[test]
fn exact_byte_bound_accepts_utf8_and_one_more_byte_keeps_previous_output() {
    let mut listing = String::from("a|1|0|");
    let remaining = MAX_LISTING_BYTES - listing.len();
    listing.push_str(&"가".repeat(remaining / 3));
    listing.push_str(&"x".repeat(remaining % 3));
    assert_eq!(listing.len(), MAX_LISTING_BYTES);
    let mut entries = Vec::new();
    assert!(parse_ls_into(&listing, &mut entries).is_ok());
    assert_eq!(entries.len(), 1);
    let previous = entries.clone();
    listing.push('x');
    assert_eq!(
        parse_ls_into(&listing, &mut entries),
        Err(ListingLimitError::TooManyBytes {
            bytes: MAX_LISTING_BYTES + 1,
        })
    );
    assert_eq!(entries, previous);
}

#[test]
fn exact_nonempty_entry_bound_accepts_blank_lines_and_rejects_one_more() {
    let mut listing: String = (0..MAX_LISTING_ENTRIES)
        .map(|i| format!("f{i}|1|0|m\r\n\r\n"))
        .collect();
    let mut entries = Vec::new();
    assert!(parse_ls_into(&listing, &mut entries).is_ok());
    assert_eq!(entries.len(), MAX_LISTING_ENTRIES);
    let previous = entries.clone();
    listing.push_str("extra|1|0|m\n");
    assert_eq!(
        parse_ls_into(&listing, &mut entries),
        Err(ListingLimitError::TooManyEntries {
            at_least: MAX_LISTING_ENTRIES + 1,
        })
    );
    assert_eq!(entries, previous);
}

#[test]
fn ignored_duplicate_and_directory_rows_cannot_bypass_the_entry_bound() {
    let mut entries = parse_ls("old|7|0|stamp");
    let previous = entries.clone();
    for row in [
        "malformed\n",
        "same|1|0|m\n",
        "../escape|1|0|m\n",
        "dir|0|1|m\n",
    ] {
        assert_eq!(
            parse_ls_into(&row.repeat(MAX_LISTING_ENTRIES + 1), &mut entries),
            Err(ListingLimitError::TooManyEntries {
                at_least: MAX_LISTING_ENTRIES + 1,
            })
        );
        assert_eq!(entries, previous);
    }
}

#[test]
fn parse_ls_into_reuses_output_vec_and_clears_old_entries() {
    let mut entries = Vec::with_capacity(4);
    entries.extend(parse_ls("old.txt|1|0|old"));
    let capacity = entries.capacity();
    parse_ls_into(
        "a.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
         sub\\dir|0|1|2026-01-01T00:00:01.0000000Z\n",
        &mut entries,
    )
    .unwrap();
    assert_eq!(entries.capacity(), capacity);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[1].name, "sub/dir");
    assert!(entries[1].is_dir);
}
