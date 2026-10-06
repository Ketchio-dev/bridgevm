//! Reading and writing the snapshot manifest's six fields.
//!
//! Parse the complete document before its fields can authorize replacement or
//! restore. Typed deserialization rejects ambiguous duplicate known fields and
//! numeric suffixes while decoding every JSON string escape.

use super::{SnapshotError, SnapshotManifest, SNAPSHOT_FORMAT_VERSION};
use serde::Deserialize;

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

#[derive(Deserialize)]
struct Fields {
    format_version: u64,
    vm_id: String,
    disk_bytes: u64,
    disk_sha256: String,
    vars_bytes: u64,
    vars_sha256: String,
}

// Flatten forces an object at the document root. Deserializing Fields directly
// also accepts a positional JSON array, which is not the manifest format.
#[derive(Deserialize)]
struct Document {
    #[serde(flatten)]
    fields: Fields,
}

pub(super) fn parse(text: &str) -> Result<SnapshotManifest, SnapshotError> {
    // Additional fields remain compatible with v1; all six known fields are
    // required at the top level and duplicate known names are refused.
    let Document { fields } = serde_json::from_str(text)
        .map_err(|error| SnapshotError::BadManifest(error.to_string()))?;
    if fields.format_version != u64::from(SNAPSHOT_FORMAT_VERSION) {
        return Err(SnapshotError::BadManifest(format!(
            "format version {}, this build reads {SNAPSHOT_FORMAT_VERSION}",
            fields.format_version
        )));
    }
    Ok(SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: fields.vm_id,
        disk_bytes: fields.disk_bytes,
        disk_sha256: fields.disk_sha256,
        vars_bytes: fields.vars_bytes,
        vars_sha256: fields.vars_sha256,
    })
}
