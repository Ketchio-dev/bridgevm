//! Capture identities before staged verification, never during recovery.

use super::{Identity, Member, Record, PIN_NAME};
use crate::snapshot_pair::{managed::private_directory, SnapshotManifest};
use std::{fs, io, path::Path};

fn member(path: &Path) -> io::Result<Member> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::other(
            "restore ownership member is not a regular file",
        ));
    }
    Ok(Member {
        identity: Identity::from_metadata(&metadata),
        len: metadata.len(),
    })
}

fn shape(stage: &Path, pinned: bool) -> io::Result<()> {
    for entry in fs::read_dir(stage)? {
        let entry = entry?;
        let name = entry.file_name();
        let known = ["disk.raw", "vars.fd", "manifest.json"]
            .iter()
            .any(|known| name == *known)
            || pinned && name == PIN_NAME;
        if !entry.file_type()?.is_file() || !known {
            return Err(io::Error::other(
                "restore ownership stage contains an unrecognized entry",
            ));
        }
    }
    Ok(())
}

pub(super) fn capture(
    root: &Path,
    stage: &Path,
    expected: (u64, u64),
    manifest: &SnapshotManifest,
    record: Identity,
) -> io::Result<Record> {
    private_directory(root, false)?
        .then_some(())
        .ok_or_else(|| io::Error::other("missing restore root"))?;
    private_directory(stage, false)?
        .then_some(())
        .ok_or_else(|| io::Error::other("missing restore stage"))?;
    let identity = Identity {
        dev: expected.0,
        ino: expected.1,
    };
    if stage.parent() != Some(root) || !identity.matches(stage)? {
        return Err(io::Error::other(
            "restore stage identity changed before sealing",
        ));
    }
    shape(stage, true)?;
    let pin = member(&stage.join(PIN_NAME))?;
    if pin.identity != record {
        return Err(io::Error::other(
            "restore ownership pin does not identify the fresh record",
        ));
    }
    Ok(Record {
        version: 1,
        root: Identity::from_metadata(&fs::symlink_metadata(root)?),
        record,
        stage: identity,
        disk: member(&stage.join("disk.raw"))?,
        vars: member(&stage.join("vars.fd"))?,
        manifest: member(&stage.join("manifest.json"))?,
        source_manifest: manifest.to_json(),
    })
}

pub(super) fn matches_stage(record: &Record, stage: &Path) -> io::Result<()> {
    if !private_directory(stage, false)? || !record.stage.matches(stage)? {
        return Err(io::Error::other(
            "restore stage no longer matches its sealed identity",
        ));
    }
    shape(stage, true)?;
    for (name, expected) in [
        ("disk.raw", record.disk),
        ("vars.fd", record.vars),
        ("manifest.json", record.manifest),
    ] {
        if member(&stage.join(name))? != expected {
            return Err(io::Error::other(
                "restore member no longer matches its sealed identity",
            ));
        }
    }
    Ok(())
}
