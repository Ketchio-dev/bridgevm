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
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[path = "snapshot_free_space.rs"]
mod free_space;
#[path = "snapshot_manifest_json.rs"]
mod manifest_json;
#[path = "snapshot_hash.rs"]
mod snapshot_hash;
use manifest_json::{escape_json, json_str, json_u64};
use snapshot_hash::sha256_file_and_size;
#[path = "snapshot_publish.rs"]
mod snapshot_publish;
#[path = "snapshot_verification.rs"]
mod snapshot_verification;
pub use snapshot_verification::verify_snapshot;

fn sha256_file(path: &Path) -> io::Result<String> {
    sha256_file_and_size(path).map(|(hash, _)| hash)
}

/// Bytes copied per read/write when streaming a large disk image.
const COPY_CHUNK: usize = 4 * 1024 * 1024;

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
    /// The volume cannot hold the copy a restore has to stage.
    InsufficientSpace {
        needed: u64,
        available: u64,
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
            SnapshotError::InsufficientSpace { needed, available } => write!(
                f,
                "restore needs {needed} bytes free to stage a copy, {available} available"
            ),
            SnapshotError::HashMismatch { file } => {
                write!(f, "{file} does not match the hash recorded in the manifest")
            }
            SnapshotError::BadManifest(why) => write!(f, "unusable snapshot manifest: {why}"),
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
    /// Serialize as JSON. Hand-rolled rather than pulling serde into this
    /// crate for six fields; the values are integers and hex/identifier
    /// strings, and `vm_id` is escaped.
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
        let format_version = json_u64(text, "format_version")?;
        if format_version != u64::from(SNAPSHOT_FORMAT_VERSION) {
            return Err(SnapshotError::BadManifest(format!(
                "format version {format_version}, this build reads {SNAPSHOT_FORMAT_VERSION}"
            )));
        }
        Ok(Self {
            format_version: SNAPSHOT_FORMAT_VERSION,
            vm_id: json_str(text, "vm_id")?,
            disk_bytes: json_u64(text, "disk_bytes")?,
            disk_sha256: json_str(text, "disk_sha256")?,
            vars_bytes: json_u64(text, "vars_bytes")?,
            vars_sha256: json_str(text, "vars_sha256")?,
        })
    }
}

/// Names inside a snapshot directory.
pub const DISK_NAME: &str = "disk.raw";
pub const VARS_NAME: &str = "vars.fd";
pub const MANIFEST_NAME: &str = "manifest.json";

/// Copy `src` to `dst` and fsync the result, returning bytes written.
///
/// Streamed rather than read wholesale: a disk image is tens of gigabytes and
/// must not be brought into memory to be copied.
fn copy_and_sync(src: &Path, dst: &Path) -> io::Result<u64> {
    // Clone first where the filesystem allows it. On APFS this is a
    // copy-on-write reference: the 64 GiB image clones in 2ms and adds no used
    // bytes, where copying it needs minutes and room for a second full copy.
    // That difference is the whole reason a restore could not run on a volume
    // with 59 GiB free.
    if free_space::clone_file(src, dst).is_some() {
        let cloned = File::open(dst)?;
        // Still fsync: the clone is metadata, and the manifest is about to
        // claim these bytes are durable.
        cloned.sync_all()?;
        return Ok(cloned.metadata()?.len());
    }
    let mut input = File::open(src)?;
    let mut output = File::create(dst)?;
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut total = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        total += n as u64;
    }
    output.sync_all()?;
    Ok(total)
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
    managed::LockedPair::open(disk, vars)?.restore(dir)
}

#[path = "managed_pair.rs"]
pub mod managed;
#[cfg(test)]
#[path = "snapshot_pair_tests.rs"]
mod snapshot_pair_tests;
