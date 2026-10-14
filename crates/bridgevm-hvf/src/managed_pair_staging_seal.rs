//! Seal before opening the staged disk for verification. An unsupported link
//! keeps the existing unsealed restore path, never weaker recovery authority.

use super::record::{Identity, Record, PIN_NAME, ROOT_NAME};
use crate::snapshot_pair::{managed::private_directory, SnapshotManifest};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub(super) fn create(
    root: &Path,
    stage: &Path,
    expected: (u64, u64),
    manifest: &SnapshotManifest,
) -> io::Result<Option<Record>> {
    create_using(
        root,
        stage,
        expected,
        manifest,
        |from, to| fs::hard_link(from, to),
        sync,
    )
}

fn sync(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn create_using(
    root: &Path,
    stage: &Path,
    expected: (u64, u64),
    manifest: &SnapshotManifest,
    link: impl FnOnce(&Path, &Path) -> io::Result<()>,
    mut sync_directory: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<Option<Record>> {
    if !private_directory(root, false)? {
        return Err(io::Error::other("missing private restore root"));
    }
    let root_identity = Identity::from_metadata(&fs::symlink_metadata(root)?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(ROOT_NAME))?;
    let record_identity = Identity::from_metadata(&file.metadata()?);
    let stage_identity = Identity {
        dev: expected.0,
        ino: expected.1,
    };
    let result = (|| {
        guard::before_link(root, stage, root_identity, stage_identity, record_identity)?;
        match link(&root.join(ROOT_NAME), &stage.join(PIN_NAME)) {
            Ok(()) => {}
            Err(error) if unsupported(&error) => {
                cleanup::unsealed(root, stage, root_identity, stage_identity, record_identity)?;
                sync_directory(root)?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        let record = Record::capture(root, stage, expected, manifest, record_identity)?;
        if record.root != root_identity {
            return Err(io::Error::other(
                "restore root changed while creating ownership record",
            ));
        }
        file.write_all(&record.encode()?)?;
        file.sync_all()?;
        // Both copied members and both names of the immutable record precede
        // the staged-verification read FD observed by the interruption gate.
        sync_directory(stage)?;
        sync_directory(root)?;
        Record::read_pinned(root, stage)?.matches_stage(stage)?;
        Ok(Some(record))
    })();
    if result.is_err() {
        drop(cleanup::failed(
            root,
            stage,
            root_identity,
            stage_identity,
            record_identity,
        ));
    }
    result
}

fn unsupported(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| code == libc::ENOTSUP || code == libc::EOPNOTSUPP)
}

#[path = "managed_pair_staging_seal_cleanup.rs"]
mod cleanup;
#[path = "managed_pair_staging_seal_guard.rs"]
mod guard;
#[cfg(test)]
#[path = "managed_pair_staging_seal_tests.rs"]
mod tests;
