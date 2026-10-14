//! Only an intact pinned attempt can authorize later-process staging cleanup.
//! A second crash during cleanup leaves incomplete proof and refuses next time.

use super::record::{Identity, Record, ROOT_NAME};
use crate::snapshot_pair::{managed::private_directory, verify_snapshot};
use std::{fs, io, path::Path};

/// True means the existing legacy admission must inspect unsealed staging.
pub(super) fn reclaim(root: &Path) -> io::Result<bool> {
    if !private_directory(root, false)? {
        return Ok(false);
    }
    if !present(&root.join(ROOT_NAME))? {
        return Ok(true);
    }
    recover(root).map_err(|error| refusal(root, error))
}

fn recover(root: &Path) -> io::Result<bool> {
    let stage = root.join("staging");
    if private_directory(&stage, false)? {
        if let Ok(record) = Record::read_pinned(root, &stage) {
            if record.stage.matches(&stage)? {
                record.matches_stage(&stage)?;
                let expected = record.expected_manifest()?;
                let actual = verify_snapshot(&stage).map_err(io::Error::other)?;
                if actual != expected {
                    return Err(io::Error::other(
                        "staged bytes differ from their sealed source",
                    ));
                }
                cleanup::clear(root, &stage, &record)?;
                return Ok(false);
            }
        }
    }
    // A completed rename can precede a crash before metadata retirement. The
    // current pair may have guest writes; only its exact directory/pin matters.
    retirement::current(root)?;
    Ok(true)
}

pub(super) fn clear_owned(
    stage: &Path,
    expected: (u64, u64),
    record: Option<&Record>,
) -> io::Result<()> {
    if !(Identity {
        dev: expected.0,
        ino: expected.1,
    })
    .matches(stage)?
    {
        return Err(io::Error::other("restore staging directory was replaced"));
    }
    match record {
        Some(record) => cleanup::clear(stage.parent().unwrap(), stage, record),
        None => super::debris::clear(stage),
    }
}

pub(super) fn retire_current(root: &Path) -> io::Result<()> {
    retirement::current(root).map_err(|error| refusal(root, error))
}

fn present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn refusal(root: &Path, error: io::Error) -> io::Error {
    io::Error::other(format!(
        "managed restore ownership at {} cannot be proven ({error}); it was left intact: \
         preserve the staging and ownership record outside managed storage before retrying",
        root.display()
    ))
}

#[path = "managed_pair_staging_owned_cleanup.rs"]
mod cleanup;
#[path = "managed_pair_staging_retirement.rs"]
mod retirement;

#[cfg(test)]
#[path = "managed_pair_staging_recovery_tests.rs"]
mod tests;
