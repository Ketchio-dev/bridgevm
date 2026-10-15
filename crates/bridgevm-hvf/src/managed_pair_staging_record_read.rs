//! Read a bounded, strict record and prove the candidate pins that same inode.

use super::{Identity, Record, LIMIT, PIN_NAME, ROOT_NAME};
use crate::snapshot_pair::{managed::private_directory, regular_file};
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};

fn private_record(metadata: &fs::Metadata, owner: u32) -> io::Result<()> {
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 2
        || metadata.len() > LIMIT
    {
        return Err(io::Error::other(
            "invalid private pinned restore ownership record",
        ));
    }
    Ok(())
}

fn object_shapes(bytes: &[u8]) -> io::Result<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    let object = value
        .as_object()
        .ok_or_else(|| io::Error::other("restore record must be an object"))?;
    for name in ["root", "record", "stage", "disk", "vars", "manifest"] {
        let member = object
            .get(name)
            .and_then(|value| value.as_object())
            .ok_or_else(|| io::Error::other("restore record identity must be an object"))?;
        if ["disk", "vars", "manifest"].contains(&name)
            && !member
                .get("identity")
                .is_some_and(|value| value.is_object())
        {
            return Err(io::Error::other(
                "restore member identity must be an object",
            ));
        }
    }
    Ok(())
}

pub(super) fn read(root: &Path, candidate: &Path) -> io::Result<Record> {
    if !private_directory(root, false)? || !private_directory(candidate, false)? {
        return Err(io::Error::other(
            "missing private restore ownership directory",
        ));
    }
    let root_metadata = fs::symlink_metadata(root)?;
    let file = regular_file::open(&root.join(ROOT_NAME))?;
    let metadata = file.metadata()?;
    private_record(&metadata, root_metadata.uid())?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::Error::other(
            "restore ownership record exceeds its size limit",
        ));
    }
    // Typed parsing rejects duplicate/unknown fields; the shape check also
    // rejects serde's otherwise accepted positional struct arrays.
    let record: Record = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    object_shapes(&bytes)?;
    record.expected_manifest()?;
    let pin = fs::symlink_metadata(candidate.join(PIN_NAME))?;
    private_record(&pin, root_metadata.uid())?;
    if record.root != Identity::from_metadata(&root_metadata)
        || record.record != Identity::from_metadata(&metadata)
        || record.record != Identity::from_metadata(&pin)
        || !record.record.matches(&root.join(ROOT_NAME))?
    {
        return Err(io::Error::other(
            "restore ownership record or pin identity changed",
        ));
    }
    Ok(record)
}
