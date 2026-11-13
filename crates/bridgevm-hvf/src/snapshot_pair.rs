//! Powered-off snapshot of the disk plus UEFI vars as one atomic pair.
//!
//! The two files are a unit: vars records which boot entry and which firmware
//! state the disk was left in, so a disk from one moment paired with vars from
//! another can boot into a state that never existed. Creation publishes a
//! complete snapshot directory atomically. Restore selects a complete managed
//! generation; interruption and product-lifecycle evidence still keep A19 open.
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

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[path = "snapshot_import_disk.rs"]
pub mod import_disk;

#[path = "snapshot_copy.rs"]
mod copy;
#[path = "snapshot_free_space.rs"]
pub(crate) mod free_space;
#[path = "snapshot_manifest_json.rs"]
mod manifest_json;
#[path = "snapshot_manifest_read.rs"]
mod manifest_read;
#[path = "snapshot_regular_file.rs"]
mod regular_file;
use copy::copy_and_sync;
#[path = "snapshot_hash.rs"]
mod snapshot_hash;
use manifest_json::escape_json;
use snapshot_hash::sha256_file_and_size;
#[path = "snapshot_publish.rs"]
mod snapshot_publish;
#[path = "snapshot_verification.rs"]
mod snapshot_verification;
pub use snapshot_verification::verify_snapshot;

fn sha256_file(path: &Path) -> io::Result<String> {
    sha256_file_and_size(path).map(|(hash, _)| hash)
}

/// Manifest format version. A restore refuses anything it does not know.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// What a snapshot refused to do, and why.
#[derive(Debug)]
pub enum SnapshotError {
    /// The VM still owns the media. Copying it now would capture a torn disk.
    VmRunning,
    /// The pair is larger than the caller allowed.
    QuotaExceeded {
        bytes: u64,
        quota: u64,
    },
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
            SnapshotError::VmRunning => {
                write!(f, "refusing to snapshot a running VM: power it off first")
            }
            SnapshotError::QuotaExceeded { bytes, quota } => {
                write!(
                    f,
                    "snapshot would write {bytes} bytes, over the {quota} byte quota"
                )
            }
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
        manifest_json::parse(text)
    }
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

#[path = "snapshot_create.rs"]
mod creation;
pub use creation::create_snapshot;

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
pub fn restore_snapshot(
    dir: &Path,
    disk: &Path,
    vars: &Path,
    vm_running: bool,
) -> Result<SnapshotManifest, SnapshotError> {
    if vm_running {
        return Err(SnapshotError::VmRunning);
    }
    runtime_exclusion::restore(dir, disk, vars)
}

#[path = "snapshot_pair_grow.rs"]
pub mod grow;
#[path = "managed_pair.rs"]
pub mod managed;
#[path = "snapshot_runtime_exclusion.rs"]
mod runtime_exclusion;
#[path = "snapshot_pair_selected.rs"]
pub mod selected;
#[path = "snapshot_pair_size.rs"]
pub mod size;
#[cfg(test)]
#[path = "snapshot_pair_tests.rs"]
mod snapshot_pair_tests;
