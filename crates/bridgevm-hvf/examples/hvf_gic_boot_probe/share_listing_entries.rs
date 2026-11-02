//! Parse an admitted guest listing using the existing name and field rules.

use super::{LsEntry, RefusedNames};

pub(super) fn parse_into(listing: &str, out: &mut Vec<LsEntry>) -> RefusedNames {
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
