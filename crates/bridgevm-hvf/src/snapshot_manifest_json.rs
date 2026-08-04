//! Reading and writing the snapshot manifest's six fields.
//!
//! Hand-rolled rather than pulling serde into this crate for one struct. Split
//! from snapshot_pair so the manifest's escaping rules can be read and tested
//! without the file-copy logic around them.

use super::SnapshotError;

pub(super) fn escape_json(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

pub(super) fn json_field<'a>(text: &'a str, key: &str) -> Result<&'a str, SnapshotError> {
    let needle = format!("\"{key}\"");
    let start = text
        .find(&needle)
        .ok_or_else(|| SnapshotError::BadManifest(format!("missing field {key}")))?;
    let after = &text[start + needle.len()..];
    let colon = after
        .find(':')
        .ok_or_else(|| SnapshotError::BadManifest(format!("field {key} has no value")))?;
    Ok(after[colon + 1..].trim_start())
}

pub(super) fn json_u64(text: &str, key: &str) -> Result<u64, SnapshotError> {
    let rest = json_field(text, key)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .map_err(|_| SnapshotError::BadManifest(format!("field {key} is not a number")))
}

pub(super) fn json_str(text: &str, key: &str) -> Result<String, SnapshotError> {
    let rest = json_field(text, key)?;
    let rest = rest
        .strip_prefix('"')
        .ok_or_else(|| SnapshotError::BadManifest(format!("field {key} is not a string")))?;
    // Walk rather than `find`, so an escaped quote inside the value does not
    // look like the end of it. escape_json emits \" and \\, so those are the
    // two sequences that can appear.
    let mut out = String::new();
    let mut chars = rest.chars();
    loop {
        match chars.next() {
            Some('"') => return Ok(out),
            Some('\\') => match chars.next() {
                Some(c @ ('"' | '\\')) => out.push(c),
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => break,
            },
            Some(c) => out.push(c),
            None => break,
        }
    }
    Err(SnapshotError::BadManifest(format!(
        "field {key} is unterminated"
    )))
}
