//! Powered-off snapshot of the disk plus UEFI vars as one atomic pair.
//!
//! The two files are a unit: vars records which boot entry and which firmware
//! state the disk was left in, so a disk from one moment paired with vars from
//! another can boot into a state that never existed. Creation publishes a
//! complete snapshot directory atomically. Restore selects a complete managed
//! generation.
//!
//! The technique is a staging directory that is filled, fsynced, and only then
//! renamed into place. `rename(2)` within a directory is atomic, so a crash
//! either leaves the staging directory (ignored, it has no `ready` marker) or
//! the finished snapshot. `fs::write` cannot give this: it truncates the
//! destination first, so an interrupted write destroys the old copy without
//! producing a new one.
//!
//! Restore verifies both hashes *before* touching either live file, because a
//! restore that fails halfway is the one case that loses data the user still
//! had.

use crate::media::lock::{lock_path_for, MediaLease as WriterLease, MediaLockError};
use managed::LockedPair;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

mod create;
pub(crate) mod files;
pub mod managed;
mod publish;
pub mod selected;

pub use create::{create_snapshot, import_disk};

/// Manifest format version. A restore refuses anything it does not know.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// What a snapshot refused to do, and why.
#[derive(Debug)]
pub enum SnapshotError {
    /// A file's content does not match the manifest.
    HashMismatch {
        file: String,
    },
    /// The manifest is missing, malformed, or a version this build cannot read.
    BadManifest(String),
    Io(io::Error),
}

impl From<io::Error> for SnapshotError {
    fn from(e: io::Error) -> Self {
        SnapshotError::Io(e)
    }
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::HashMismatch { file } => {
                write!(f, "{file} does not match the hash recorded in the manifest")
            }
            SnapshotError::BadManifest(why) => write!(f, "unusable snapshot manifest: {why}"),
            SnapshotError::Io(e) if e.raw_os_error() == Some(libc::ENOSPC) => {
                write!(
                    f,
                    "No space left on device; free space on your Mac and try again."
                )
            }
            SnapshotError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// What was captured, and enough to prove it was captured intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub format_version: u32,
    pub vm_id: String,
    pub disk_bytes: u64,
    pub disk_sha256: String,
    pub vars_bytes: u64,
    pub vars_sha256: String,
}

impl SnapshotManifest {
    /// Serialize with stable formatting; strings are escaped as JSON.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n  \"format_version\": {},\n  \"vm_id\": \"{}\",\n",
                "  \"disk_bytes\": {},\n  \"disk_sha256\": \"{}\",\n",
                "  \"vars_bytes\": {},\n  \"vars_sha256\": \"{}\"\n}}\n"
            ),
            self.format_version,
            escape_json(&self.vm_id),
            self.disk_bytes,
            self.disk_sha256,
            self.vars_bytes,
            self.vars_sha256,
        )
    }

    pub fn from_json(text: &str) -> Result<Self, SnapshotError> {
        parse_manifest(text)
    }
}

// Parse the complete document before its fields can authorize replacement or
// restore. Typed deserialization rejects ambiguous duplicate known fields and
// numeric suffixes while decoding every JSON string escape.

fn escape_json(s: &str) -> String {
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

fn parse_manifest(text: &str) -> Result<SnapshotManifest, SnapshotError> {
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

/// Names inside a snapshot directory.
pub const DISK_NAME: &str = "disk.raw";
pub const VARS_NAME: &str = "vars.fd";
pub const MANIFEST_NAME: &str = "manifest.json";

/// The shared-folder sync state the VM keeps beside its vars, as `<vars><suffix>`: the versions
/// it last synced, then the marks of transfers under way. It describes the guest disk it was
/// written with. A restore rolls that disk back while the Mac folder stays current, so the state
/// would take the guest's older copy for an edit and fetch it over the Mac file: a restore removes
/// it (versions first) and the next start begins without one, the Mac copy winning. A copied
/// bundle or a Time Machine restore of the bundle moves the state with its disk.
pub const SHARE_STATE_SUFFIXES: [&str; 2] = [".share-sync", ".share-sync-puts"];

/// Removes the share state beside `vars`, durably. Missing is fine; any other error is returned.
fn remove_share_state(vars: &Path) -> io::Result<()> {
    let mut removed = false;
    for suffix in SHARE_STATE_SUFFIXES {
        let mut path = vars.as_os_str().to_owned();
        path.push(suffix);
        match fs::remove_file(&path) {
            Ok(()) => removed = true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match vars.parent() {
        Some(dir) if removed => File::open(dir)?.sync_all(),
        _ => Ok(()),
    }
}

/// fsync a directory, so a rename inside it survives a crash.
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Write `bytes` to `path` atomically: temp file, fsync, rename, fsync parent.
pub fn write_file_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_dir(parent)
}

fn staging_path(dest: &Path) -> PathBuf {
    let parent = dest.parent().unwrap_or(Path::new("."));
    parent.join(format!(
        ".{}.staging",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ))
}

/// Restore by atomically selecting a complete managed generation.
/// Disk and vars are logical identities; read their current files through
/// managed::LockedPair while retaining ownership, not through the originals.
/// An error after publication can still leave the complete new pair selected.
/// Both writer leases are held until the new generation is selected.
pub fn restore_snapshot(
    dir: &Path,
    disk: &Path,
    vars: &Path,
) -> Result<SnapshotManifest, SnapshotError> {
    powered_off(disk, vars, "restore")?.pair.restore(dir)
}

// Powered-off create, restore and create sizing exclude an owned runtime.
//
// An owned runtime (`bridgevm-hvf-runtime`'s `prepare`) holds the image
// writer leases in `media::lock` from start to exit, but the pair lease that
// `LockedPair` takes is held only while a guest generation runs. Before the
// first generation and between reboots the writer leases are the only sign
// that a runtime owns the media. A powered-off operation therefore takes
// them first, before any destination or media is touched, and keeps them
// until it finishes: the operation is refused while a runtime is alive, and
// a runtime cannot be prepared on that media while the operation runs.
//
// Only an existing lease file is taken. A runtime creates it before locking
// and never removes it, so its absence means no runtime holds the image now;
// creating one would leave a stray file beside every source, including the
// members of a snapshot, which then could never be replaced. A runtime that
// starts mid-operation on such never-run media is still refused at its first
// generation, which cannot open the pair this operation owns.
//
// This must stay out of `LockedPair::open`: every guest generation opens the
// pair while its runtime holds these writer leases.

/// Field order is drop order: the pair lease goes before the writer leases,
/// so a runtime admitted afterwards never finds the pair still owned.
struct PoweredOffPair {
    pair: LockedPair,
    _writers: [Option<WriterLease>; 2],
}

/// `operation` names this process to a runtime refused while it holds media.
fn powered_off(disk: &Path, vars: &Path, operation: &str) -> io::Result<PoweredOffPair> {
    let holder = format!("powered-off snapshot {operation}");
    let writers = [writer_lease(disk, &holder)?, writer_lease(vars, &holder)?];
    Ok(PoweredOffPair {
        pair: LockedPair::open(disk, vars)?,
        _writers: writers,
    })
}

fn writer_lease(image: &Path, holder: &str) -> io::Result<Option<WriterLease>> {
    match fs::symlink_metadata(lock_path_for(image)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    WriterLease::acquire(image, holder)
        .map(Some)
        .or_else(writer_refusal)
}

/// A read-only volume cannot hold a runtime's writer lease either (it opens
/// the file for writing), so media read from one, such as an import source,
/// is not refused.
fn writer_refusal(error: MediaLockError) -> io::Result<Option<WriterLease>> {
    let (kind, why) = match &error {
        MediaLockError::Io { source, .. } if source.raw_os_error() == Some(libc::EROFS) => {
            return Ok(None)
        }
        MediaLockError::Held { .. } => (
            io::ErrorKind::WouldBlock,
            "media is in use by a running VM or another snapshot operation",
        ),
        MediaLockError::Io { source, .. } => (source.kind(), "media writer lease unavailable"),
    };
    Err(io::Error::new(kind, format!("{why}: {error}")))
}

/// Shared by restore verification and destructive replacement admission.
const MANIFEST_LIMIT: u64 = 64 * 1024;

/// Read bounded snapshot metadata without following a link or blocking on a FIFO.
fn read_manifest(path: &Path) -> Result<SnapshotManifest, SnapshotError> {
    let text = read_manifest_text(path)
        .map_err(|error| SnapshotError::BadManifest(format!("cannot read manifest: {error}")))?;
    SnapshotManifest::from_json(&text)
}

fn read_manifest_text(path: &Path) -> io::Result<String> {
    let file = files::open_regular(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > MANIFEST_LIMIT {
        return Err(io::Error::other("manifest exceeds the 64 KiB size limit"));
    }
    let mut text = String::new();
    file.take(MANIFEST_LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err(io::Error::other("manifest exceeds the 64 KiB size limit"));
    }
    Ok(text)
}

/// Read and verify a snapshot without restoring it.
pub fn verify_snapshot(dir: &Path) -> Result<SnapshotManifest, SnapshotError> {
    inspect_snapshot(dir, true)
}

/// Without `hash`, only the manifest and each member's type and size are
/// checked: a restore hashes its staged copy, the bytes it publishes, instead.
fn inspect_snapshot(dir: &Path, hash: bool) -> Result<SnapshotManifest, SnapshotError> {
    let manifest = read_manifest(&dir.join(MANIFEST_NAME))?;
    for (name, want_hash, want_bytes) in [
        (DISK_NAME, &manifest.disk_sha256, manifest.disk_bytes),
        (VARS_NAME, &manifest.vars_sha256, manifest.vars_bytes),
    ] {
        verify_member(
            &dir.join(name),
            hash.then_some(want_hash.as_str()),
            want_bytes,
        )?;
    }
    Ok(manifest)
}

/// Refuse an unexpected member type or size before hashing its contents.
fn verify_member(
    path: &Path,
    want_hash: Option<&str>,
    want_bytes: u64,
) -> Result<(), SnapshotError> {
    let file = files::open_regular(path)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mismatch = |bytes| {
        SnapshotError::BadManifest(format!(
            "{name} contains {bytes} bytes, manifest declares {want_bytes}"
        ))
    };
    let measured = file.metadata()?.len();
    if measured != want_bytes {
        return Err(mismatch(measured));
    }
    let Some(want_hash) = want_hash else {
        return Ok(());
    };
    // A growing member gets at most one byte beyond its declared size read;
    // a shrinking one is rejected by the byte count from the same descriptor.
    let (hash, bytes) = files::sha256_reader(file.take(want_bytes.saturating_add(1)))?;
    if bytes != want_bytes {
        return Err(mismatch(bytes));
    }
    if hash != want_hash {
        return Err(SnapshotError::HashMismatch {
            file: name.into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod manifest_tests;
#[cfg(test)]
mod tests;
