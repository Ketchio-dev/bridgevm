//! A restore seal binds its own inode to the original copied generation.
//!
//! Seal before opening the staged disk for verification. An unsupported link
//! keeps the existing unsealed restore path, never weaker recovery authority.
//!
//! Only an intact pinned attempt can authorize later-process staging cleanup.
//! A second crash during cleanup leaves incomplete proof and refuses next time.

use super::private_directory;
use crate::snapshot_pair::files::open_regular;
use crate::snapshot_pair::{verify_snapshot, SnapshotManifest};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

const ROOT_NAME: &str = ".restore-stage-owner";
const PIN_NAME: &str = ".bridgevm-restore-owner";
const LIMIT: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    dev: u64,
    ino: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Member {
    identity: Identity,
    len: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    version: u32,
    root: Identity,
    record: Identity,
    stage: Identity,
    disk: Member,
    vars: Member,
    manifest: Member,
    source_manifest: String,
}

impl Identity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }

    fn matches(self, path: &Path) -> io::Result<bool> {
        Ok(Self::from_metadata(&fs::symlink_metadata(path)?) == self)
    }
}

impl Record {
    fn encode(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::other(
                "restore ownership record exceeds its size limit",
            ));
        }
        Ok(bytes)
    }

    /// Capture identities before staged verification, never during recovery.
    fn capture(
        root: &Path,
        stage: &Path,
        expected: (u64, u64),
        manifest: &SnapshotManifest,
        record: Identity,
    ) -> io::Result<Self> {
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

    /// Read a bounded, strict record and prove the candidate pins that same inode.
    fn read_pinned(root: &Path, candidate: &Path) -> io::Result<Self> {
        if !private_directory(root, false)? || !private_directory(candidate, false)? {
            return Err(io::Error::other(
                "missing private restore ownership directory",
            ));
        }
        let root_metadata = fs::symlink_metadata(root)?;
        let file = open_regular(&root.join(ROOT_NAME))?;
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

    fn matches_stage(&self, stage: &Path) -> io::Result<()> {
        if !private_directory(stage, false)? || !self.stage.matches(stage)? {
            return Err(io::Error::other(
                "restore stage no longer matches its sealed identity",
            ));
        }
        shape(stage, true)?;
        for (name, expected) in [
            ("disk.raw", self.disk),
            ("vars.fd", self.vars),
            ("manifest.json", self.manifest),
        ] {
            if member(&stage.join(name))? != expected {
                return Err(io::Error::other(
                    "restore member no longer matches its sealed identity",
                ));
            }
        }
        Ok(())
    }

    fn expected_manifest(&self) -> io::Result<SnapshotManifest> {
        let manifest =
            SnapshotManifest::from_json(&self.source_manifest).map_err(io::Error::other)?;
        if self.version != 1 || manifest.to_json() != self.source_manifest {
            return Err(io::Error::other(
                "invalid restore ownership manifest or version",
            ));
        }
        Ok(manifest)
    }
}

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
        before_link(root, stage, root_identity, stage_identity, record_identity)?;
        match link(&root.join(ROOT_NAME), &stage.join(PIN_NAME)) {
            Ok(()) => {}
            Err(error) if unsupported(&error) => {
                cleanup_unsealed(root, stage, root_identity, stage_identity, record_identity)?;
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
        drop(cleanup_failed(
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

// Same-attempt metadata cleanup uses identities held before any record write.

fn check_directories(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
) -> io::Result<()> {
    if !root_id.matches(root)? || !stage_id.matches(stage)? {
        return Err(io::Error::other(
            "restore ownership directories changed during sealing",
        ));
    }
    Ok(())
}

fn remove_owned(path: &Path, identity: Identity) -> io::Result<()> {
    if !identity.matches(path)? {
        return Err(io::Error::other("restore ownership metadata was replaced"));
    }
    fs::remove_file(path)
}

fn cleanup_unsealed(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    check_directories(root, stage, root_id, stage_id)?;
    match fs::symlink_metadata(stage.join(PIN_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(io::Error::other(
                "unsupported restore ownership link left a pin",
            ))
        }
        Err(error) => return Err(error),
    }
    remove_owned(&root.join(ROOT_NAME), record_id)
}

fn cleanup_failed(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    check_directories(root, stage, root_id, stage_id)?;
    match fs::symlink_metadata(stage.join(PIN_NAME)) {
        Ok(_) => {
            remove_owned(&stage.join(PIN_NAME), record_id)?;
            sync(stage)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::symlink_metadata(root.join(ROOT_NAME)) {
        Ok(_) => remove_owned(&root.join(ROOT_NAME), record_id)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    sync(root)
}

/// Admit identities before probing links, without rejecting filesystem metadata.
fn before_link(
    root: &Path,
    stage: &Path,
    root_id: Identity,
    stage_id: Identity,
    record_id: Identity,
) -> io::Result<()> {
    if !private_directory(root, false)?
        || !private_directory(stage, false)?
        || stage.parent() != Some(root)
        || !root_id.matches(root)?
        || !stage_id.matches(stage)?
        || !record_id.matches(&root.join(ROOT_NAME))?
    {
        return Err(io::Error::other(
            "restore ownership identities changed before the link probe",
        ));
    }
    Ok(())
}

/// A previous attempt's staging or ownership record: all that reclaim can touch.
pub(super) fn previous_attempt(root: &Path) -> io::Result<bool> {
    Ok(present(&root.join("staging"))? || present(&root.join(ROOT_NAME))?)
}

/// True means the existing legacy admission must inspect unsealed staging.
pub(super) fn reclaim_sealed(root: &Path) -> io::Result<bool> {
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
                clear_sealed(root, &stage, &record)?;
                return Ok(false);
            }
        }
    }
    // A completed rename can precede a crash before metadata retirement. The
    // current pair may have guest writes; only its exact directory/pin matters.
    retire_current_record(root)?;
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
        Some(record) => clear_sealed(stage.parent().unwrap(), stage, record),
        None => super::staging::clear_debris(stage),
    }
}

pub(super) fn retire_current(root: &Path) -> io::Result<()> {
    retire_current_record(root).map_err(|error| refusal(root, error))
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

/// Validate everything before unlinking only the captured attempt's members.
fn clear_sealed(root: &Path, stage: &Path, record: &Record) -> io::Result<()> {
    let observed = Record::read_pinned(root, stage)?;
    if observed.record != record.record || !record.root.matches(root)? {
        return Err(io::Error::other("restore ownership record was replaced"));
    }
    record.matches_stage(stage)?;
    // Recheck after any recovery hashing, before the first destructive action.
    for (name, member) in [
        ("disk.raw", record.disk),
        ("vars.fd", record.vars),
        ("manifest.json", record.manifest),
    ] {
        if !record.stage.matches(stage)? || !member.identity.matches(&stage.join(name))? {
            return Err(io::Error::other("restore member changed before cleanup"));
        }
        fs::remove_file(stage.join(name))?;
    }
    if !record.stage.matches(stage)? || !record.record.matches(&stage.join(PIN_NAME))? {
        return Err(io::Error::other("restore pin changed before cleanup"));
    }
    fs::remove_file(stage.join(PIN_NAME))?;
    // An unexpected late entry prevents rmdir; never recurse into it.
    fs::remove_dir(stage)?;
    retire_record(root, record)
}

/// Retire metadata only; writable current media is never rehashed or removed.
fn retire_current_record(root: &Path) -> io::Result<()> {
    if !present(&root.join(ROOT_NAME))? {
        return Ok(());
    }
    let current = root.join("current");
    let owned = Record::read_pinned(root, &current)?;
    if !owned.stage.matches(&current)? {
        return Err(io::Error::other(
            "ownership record does not identify current generation",
        ));
    }
    retire_record(root, &owned)
}

fn retire_record(root: &Path, owned: &Record) -> io::Result<()> {
    if !owned.root.matches(root)? || !owned.record.matches(&root.join(ROOT_NAME))? {
        return Err(io::Error::other(
            "restore ownership root or record was replaced",
        ));
    }
    // A successful publisher can leave old staging after best-effort cleanup.
    // Even without a manifest or pin, it must never become legacy copy debris
    // just because retirement forgot the exchange on this or a later attempt.
    if present(&root.join("staging"))? {
        return Err(io::Error::other(
            "unselected staging may be the previous published generation",
        ));
    }
    fs::remove_file(root.join(ROOT_NAME))?;
    File::open(root)?.sync_all()
}

#[cfg(test)]
mod tests;
