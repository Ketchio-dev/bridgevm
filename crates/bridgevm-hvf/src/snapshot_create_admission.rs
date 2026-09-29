//! Admit only snapshot outputs that creation may replace or clear.
//!
//! Publication exchanges the destination with staging and then deletes what
//! was swapped out, so anything admitted here is eventually removed. Only an
//! absent path, an empty directory or a previous snapshot qualifies, and an
//! earlier attempt's staging is cleared only when it holds nothing but files
//! creation itself writes there.

use super::{snapshot_publish, SnapshotManifest, DISK_NAME, MANIFEST_NAME, VARS_NAME};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// write_file_atomically's temporary name for MANIFEST_NAME.
const MANIFEST_TEMPORARY: &str = ".manifest.json.tmp";
/// Far above any manifest this build writes.
const MANIFEST_LIMIT: u64 = 64 * 1024;

/// Hashes are not checked: replacing a corrupt snapshot is intended.
pub(super) fn admit_destination(destination: &Path) -> io::Result<()> {
    let refuse = |why: String| refusal("destination", destination, why);
    if !present_directory(destination).map_err(refuse)? {
        return Ok(());
    }
    let names = listed(destination, &[DISK_NAME, VARS_NAME, MANIFEST_NAME]).map_err(refuse)?;
    match names.len() {
        0 => return Ok(()),
        3 => {}
        _ => return Err(refuse("is not a complete BridgeVM snapshot".into())),
    }
    let manifest = read_manifest(&destination.join(MANIFEST_NAME)).map_err(refuse)?;
    for (name, declared) in [
        (DISK_NAME, manifest.disk_bytes),
        (VARS_NAME, manifest.vars_bytes),
    ] {
        let actual = fs::symlink_metadata(destination.join(name))
            .map_err(|error| refuse(format!("cannot be inspected: {error}")))?
            .len();
        if actual != declared {
            return Err(refuse(format!(
                "holds a {actual}-byte {name} where its manifest declares {declared}"
            )));
        }
    }
    Ok(())
}

pub(super) fn clear_staging(staging: &Path) -> io::Result<()> {
    let refuse = |why: String| refusal("staging directory", staging, why);
    if !present_directory(staging).map_err(refuse)? {
        return Ok(());
    }
    let allowed = [DISK_NAME, VARS_NAME, MANIFEST_NAME, MANIFEST_TEMPORARY];
    for name in listed(staging, &allowed).map_err(refuse)? {
        fs::remove_file(staging.join(name))?;
    }
    // Not remove_dir_all: an entry that arrived after listing is kept.
    fs::remove_dir(staging)
}

/// Re-admit after staging, so a destination that changed meanwhile is kept.
pub(super) fn publish(staging: &Path, destination: &Path) -> io::Result<()> {
    if let Err(error) = admit_destination(destination) {
        // Staging holds only this attempt's files; do not strand a full copy.
        let _ = clear_staging(staging);
        return Err(error);
    }
    snapshot_publish::publish(staging, destination)
}

fn present_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err("is not a directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot be inspected: {error}")),
    }
}

/// Every entry must be a regular file whose name is in `allowed`.
fn listed(dir: &Path, allowed: &[&'static str]) -> Result<Vec<&'static str>, String> {
    let unreadable = |error: io::Error| format!("cannot be listed: {error}");
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let Some(name) = allowed.iter().find(|name| entry.file_name() == **name) else {
            let found = entry.file_name();
            return Err(format!("contains {found:?}, which BridgeVM did not write"));
        };
        if !entry.file_type().map_err(unreadable)?.is_file() {
            return Err(format!("contains {name}, which is not a regular file"));
        }
        names.push(*name);
    }
    Ok(names)
}

fn read_manifest(path: &Path) -> Result<SnapshotManifest, String> {
    let mut text = String::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|file| file.take(MANIFEST_LIMIT + 1).read_to_string(&mut text))
        .map_err(|error| format!("has an unreadable manifest: {error}"))?;
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err("has an oversized manifest".into());
    }
    SnapshotManifest::from_json(&text)
        .map_err(|error| format!("has an unusable manifest ({error})"))
}

/// The managed in-app path has no output choice, so removal is offered too.
fn refusal(what: &str, path: &Path, why: String) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "snapshot {what} {} {why}; it was left intact: {}",
            path.display(),
            "remove or rename it, or choose a new output path"
        ),
    )
}
