//! Create a snapshot while preserving its logical and selected source media.
//!
//! Creation fills `.NAME.staging` beside the destination and publishes it
//! with a rename or an exchange. Before staging is touched, the destination is
//! admitted and the media members of the destination and of staging are
//! leased; the manifest is written last, so only staging that holds one can be
//! a completed snapshot. The destination is admitted again right before
//! publication, which is the check that keeps what arrived during the copy.
//! A create also clears sibling staging that a killed create left behind.

use super::files::{copy_and_sync, sha256_file};
use super::publish::publish;
use super::{
    managed, powered_off, read_manifest, staging_path, sync_dir, write_file_atomically,
    writer_lease, SnapshotError, SnapshotManifest, DISK_NAME, MANIFEST_LIMIT, MANIFEST_NAME,
    SNAPSHOT_FORMAT_VERSION, VARS_NAME,
};
use crate::media_lease::MediaLease;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Internal durable boundaries in snapshot creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreateStage {
    DiskSynced,
    VarsSynced,
    ManifestPublished,
    StagingDirectorySynced,
    SnapshotPublished,
}

/// Capture `disk` and `vars` into `dest` as one atomic pair.
///
/// A running VM's writer leases refuse the capture: between guest generations
/// nothing else shows that it owns the media.
pub fn create_snapshot(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
) -> Result<SnapshotManifest, SnapshotError> {
    create_snapshot_using(disk, vars, dest, vm_id, |_| {})
}

fn create_snapshot_using(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    observe: impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    create_stopped(disk, vars, dest, vm_id, copy_and_sync, publish, observe)
}

/// Create from the selected stopped pair under both source and destination ownership.
///
/// `copy` stages one file and `publish` swaps staging in; tests substitute
/// a streaming copy, or either one failing.
fn create_stopped(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    mut observe: impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    // Refuse before writing anything, including while a runtime owns the media.
    let owner = powered_off(disk, vars, "create")?;
    let (selected_disk, selected_vars) = owner.pair.paths()?;
    let logical_disk = fs::canonicalize(disk)?;
    let logical_vars = fs::canonicalize(vars)?;
    let (disk, vars) = (selected_disk.as_path(), selected_vars.as_path());

    let dest = prepare_destination(dest, [&logical_disk, &logical_vars, disk, vars])?;
    let (_members, staging) = claim_staging(&dest)?;
    let manifest = staging.fill([disk, vars], vm_id, copy, &mut observe)?;
    // Never remove the previous snapshot before its replacement is published.
    staging.publish(&dest, publish)?;
    observe(CreateStage::SnapshotPublished);
    Ok(manifest)
}

/// Resolve a snapshot destination without permitting source overlap.
fn prepare_destination(dest: &Path, sources: [&Path; 4]) -> io::Result<PathBuf> {
    let name = dest
        .file_name()
        .ok_or_else(|| io::Error::other("snapshot destination needs a directory name"))?;
    let parent = dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    // Resolve parent aliases while retaining the final entry: publication must
    // still refuse a destination symlink rather than replace its target.
    let destination = fs::canonicalize(parent)?.join(name);
    for output in [&destination, &staging_path(&destination)] {
        let resolved = match fs::canonicalize(output) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => output.to_path_buf(),
            Err(error) => return Err(error),
        };
        if sources.iter().any(|source| source.starts_with(&resolved)) {
            return Err(io::Error::other("snapshot output overlaps source media"));
        }
    }
    Ok(destination)
}

/// Admit the destination, own its and staging's media members, then claim
/// fresh staging.
///
/// The admission here only refuses before anything is copied; publication
/// admits again. Replacing or reclaiming a directory must own its members:
/// a reader (another export's source, a runtime) holds their disk/vars keys,
/// and another create of this destination holds those of its staging. The
/// keys of an existing directory's members are canonical, so a case or
/// Unicode alias of the same name meets the same keys.
fn claim_staging(destination: &Path) -> io::Result<(MediaLease, OwnedStaging)> {
    admit_destination(destination)?;
    let staged = staging_path(destination);
    let mut lease = MediaLease::acquire([])?;
    own_members(&mut lease, destination)?;
    // Refusal to own a leased member must precede all staging reclamation.
    own_members(&mut lease, &staged)?;
    reclaim_staging(&staged)?;
    let staging = OwnedStaging::create(&staged)?;
    // Reserve the fresh member path keys before copying either file; for
    // staging that existed above they are already held.
    own_members(&mut lease, &staged)?;
    reclaim_abandoned(&staged);
    Ok((lease, staging))
}

/// Best effort: clear what earlier killed creates left beside this one.
///
/// A killed create leaves `.N.staging` and, from the app, its empty `N`
/// reservation; later creates choose new names, so their own reclaim never
/// meets it. A sibling is cleared only when `N` is absent or an empty
/// directory, no create owns its member keys and `reclaim_staging` finds only
/// creation's own unfinished files; then the empty `N` is removed. Empty
/// staging is skipped: a live create's is empty until it owns those keys.
/// Anything else is kept, and nothing here fails this create.
fn reclaim_abandoned(own: &Path) {
    let (Some(parent), Some(own)) = (own.parent(), own.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for name in entries.flatten().map(|entry| entry.file_name()) {
        let Some(reserved) = name
            .as_bytes()
            .strip_prefix(b".")
            .and_then(|rest| rest.strip_suffix(b".staging"))
            .filter(|reserved| !matches!(*reserved, b"" | b"." | b".."))
        else {
            continue;
        };
        let reservation = parent.join(OsStr::from_bytes(reserved));
        let staging = parent.join(&name);
        if name.as_os_str() == own || !absent_or_empty(&reservation) || empty(&staging) {
            continue;
        }
        let Ok(mut lease) = MediaLease::acquire([]) else {
            return;
        };
        if own_members(&mut lease, &staging).is_ok() && reclaim_staging(&staging).is_ok() {
            let _ = fs::remove_dir(&reservation);
        }
    }
}

fn absent_or_empty(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) => metadata.is_dir() && empty(path),
        Err(error) => error.kind() == io::ErrorKind::NotFound,
    }
}

fn empty(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_none())
}

fn own_members(lease: &mut MediaLease, directory: &Path) -> io::Result<()> {
    if !present_directory(directory).map_err(|why| refusal("media directory", directory, why))? {
        return Ok(());
    }
    lease
        .extend([
            directory.join(DISK_NAME).as_path(),
            directory.join(VARS_NAME).as_path(),
        ])
        .map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                error
            } else {
                refusal("media directory", directory, error.to_string())
            }
        })
}

// Admit only snapshot destinations that creation may replace.
//
// Publication exchanges the destination with staging and then deletes what
// was swapped out, so anything admitted here is eventually removed. Only an
// absent path, an empty directory or a previous snapshot qualifies.

/// Hashes are not checked: replacing a corrupt snapshot is intended.
fn admit_destination(destination: &Path) -> io::Result<()> {
    let refuse = |why: String| refusal("destination", destination, why);
    if !present_directory(destination).map_err(refuse)? {
        return Ok(());
    }
    let members = [DISK_NAME, VARS_NAME, MANIFEST_NAME];
    let names = listed(destination, &members, false).map_err(refuse)?;
    match names.len() {
        0 => return Ok(()),
        3 => {}
        _ => return Err(refuse("is not a complete BridgeVM snapshot".into())),
    }
    let manifest = read_manifest(&destination.join(MANIFEST_NAME))
        .map_err(|error| refuse(format!("has an unusable manifest ({error})")))?;
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

fn present_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err("is not a directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot be inspected: {error}")),
    }
}

/// Every entry must be a regular file named in `allowed` or, with `companions`,
/// the "._" AppleDouble file in which exFAT keeps such a file's attributes.
fn listed(dir: &Path, allowed: &[&str], companions: bool) -> Result<Vec<OsString>, String> {
    let unreadable = |error: io::Error| format!("cannot be listed: {error}");
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let name = entry.file_name();
        let owner = match name.as_bytes().strip_prefix(b"._") {
            Some(owner) if companions => owner,
            _ => name.as_bytes(),
        };
        if !allowed.iter().any(|allowed| allowed.as_bytes() == owner) {
            return Err(format!("contains {name:?}, which is not a snapshot file"));
        }
        if !entry.file_type().map_err(unreadable)?.is_file() {
            return Err(format!("contains {name:?}, which is not a regular file"));
        }
        names.push(name);
    }
    Ok(names)
}

/// The app chooses its snapshot path. Moving an entry aside keeps it recoverable.
fn refusal(what: &str, path: &Path, why: String) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "snapshot {what} {} {why}. The {what} was left intact. Choose a new output path or move it aside.",
            path.display()
        ),
    )
}

// Clear an earlier attempt's staging only when it holds creation's own files.

/// write_file_atomically's temporary name for MANIFEST_NAME.
const MANIFEST_TEMPORARY: &str = ".manifest.json.tmp";

fn clear_staging(staging: &Path) -> io::Result<()> {
    clear_staging_then(staging, || {})
}

/// Tests act between listing and removal through `after_listing`.
fn clear_staging_then(staging: &Path, after_listing: impl FnOnce()) -> io::Result<()> {
    let refuse = |why: String| refusal("staging directory", staging, why);
    if !present_directory(staging).map_err(refuse)? {
        return Ok(());
    }
    let allowed = [DISK_NAME, VARS_NAME, MANIFEST_NAME, MANIFEST_TEMPORARY];
    let names = listed(staging, &allowed, true).map_err(refuse)?;
    after_listing();
    for name in names {
        // On exFAT, unlinking a file also unlinks its "._" companion.
        match fs::remove_file(staging.join(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            removed => removed?,
        }
    }
    // Not remove_dir_all: an entry that arrived after listing is kept.
    fs::remove_dir(staging)
}

/// Reclaim pre-existing staging only when it provably never completed.
///
/// `.NAME.staging` is also a name a user may export to, and a completed export
/// there is indistinguishable from an attempt that finished staging and then
/// died before publication. Creation renames manifest.json into staging last,
/// so only its absence proves no snapshot completed there. Staging holding it
/// is kept for the user to move or remove; this attempt's own staging is still
/// cleared by `clear_staging` after a failure.
///
/// Runs under the member leases, before staging is claimed.
fn reclaim_staging(staging: &Path) -> io::Result<()> {
    let refuse = |why: String| refusal("staging directory", staging, why);
    if !present_directory(staging).map_err(refuse)? {
        return Ok(());
    }
    match fs::symlink_metadata(staging.join(MANIFEST_NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => clear_staging(staging),
        Ok(_) => Err(refuse(format!(
            "holds a published {MANIFEST_NAME}, so it may be a completed snapshot \
             (such as an earlier export) rather than an unfinished attempt"
        ))),
        Err(error) => Err(refuse(format!("cannot be inspected: {error}"))),
    }
}

/// The staging directory one create attempt made, known by device and inode.
///
/// A later attempt never reclaims staging that holds a published manifest, so
/// an attempt whose filling or publication fails clears its own staging. It
/// does so only while the path still names the directory it created: a swap
/// or rename that took effect moved that directory, leaving the replaced
/// snapshot or nothing there, and a foreign replacement is not this attempt's.
#[derive(Debug)]
struct OwnedStaging {
    path: PathBuf,
    identity: (u64, u64),
}

impl OwnedStaging {
    /// Create staging empty and record its identity before anything else runs.
    fn create(path: &Path) -> io::Result<Self> {
        fs::create_dir(path)?;
        let identity = identity(path)?;
        let path = path.to_path_buf();
        Ok(Self { path, identity })
    }

    /// Staging was created empty under the member leases, so a failure leaves
    /// only this attempt's partial copy there. Clearing keeps anything else.
    fn fill(
        &self,
        sources: [&Path; 2],
        vm_id: &str,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
        observe: &mut impl FnMut(CreateStage),
    ) -> Result<SnapshotManifest, SnapshotError> {
        fill_staging(sources, &self.path, vm_id, copy, observe)
            .inspect_err(|_| drop(self.discard()))
    }

    /// Re-admit after staging, so a destination that changed meanwhile is kept.
    /// `publish` is `publish::publish` outside tests.
    fn publish(
        &self,
        destination: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        admit_destination(destination)
            .and_then(|()| publish(&self.path, destination))
            .map_err(|error| self.refusal(error))
    }

    /// Clearing follows debris rules, so content that arrived meanwhile stays.
    /// Returns why this attempt's staging was kept, or None when it is gone.
    fn discard(&self) -> Option<String> {
        match identity(&self.path) {
            Ok(found) if found == self.identity => clear_staging(&self.path)
                .err()
                .map(|error| format!("it could not be cleared ({error})")),
            Ok(found) => Some(format!(
                "the path now names another directory (device and inode {found:?}, created as {:?})",
                self.identity
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => Some(format!("it cannot be inspected ({error})")),
        }
    }

    /// A failed publication names staging it had to keep: a kept manifest makes
    /// the next create refuse, so the reason belongs in this refusal.
    fn refusal(&self, error: io::Error) -> io::Error {
        let guidance = match error.kind() {
            io::ErrorKind::Unsupported => UNSUPPORTED_GUIDANCE,
            _ => "",
        };
        match self.discard() {
            None if guidance.is_empty() => error,
            None => io::Error::new(error.kind(), format!("{error}{guidance}")),
            Some(kept) => io::Error::new(
                error.kind(),
                format!(
                    "{error}{guidance}; this attempt's staging {} was kept: {kept}",
                    self.path.display()
                ),
            ),
        }
    }
}

fn identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

/// Without an exchange, only an absent destination or an APFS volume can publish.
const UNSUPPORTED_GUIDANCE: &str =
    "; choose an output path that does not exist yet, or one on an APFS volume";

/// Stage a copied pair, then hash it and publish its manifest last.
fn fill_staging(
    [disk, vars]: [&Path; 2],
    staging: &Path,
    vm_id: &str,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    observe: &mut impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    let disk_bytes = copy(disk, &staging.join(DISK_NAME))?;
    observe(CreateStage::DiskSynced);
    let vars_bytes = copy(vars, &staging.join(VARS_NAME))?;
    observe(CreateStage::VarsSynced);

    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: vm_id.to_string(),
        disk_bytes,
        disk_sha256: sha256_file(&staging.join(DISK_NAME))?,
        vars_bytes,
        vars_sha256: sha256_file(&staging.join(VARS_NAME))?,
    };
    // The manifest is written last and is what makes the directory valid.
    write_file_atomically(&staging.join(MANIFEST_NAME), encode(&manifest)?.as_bytes())?;
    observe(CreateStage::ManifestPublished);
    sync_dir(staging)?;
    observe(CreateStage::StagingDirectorySynced);
    Ok(manifest)
}

/// Never publish a manifest the shared reader would refuse.
fn encode(manifest: &SnapshotManifest) -> Result<String, SnapshotError> {
    let text = manifest.to_json();
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot manifest exceeds the 64 KiB size limit",
        )
        .into());
    }
    Ok(text)
}

/// Disk-only import: reuse selected pair capture, or own an unmanaged RAW copy.
///
/// A managed disk brings its selected vars; only an unmanaged disk needs a seed.
/// Returns true when vars.fd was captured with the disk.
pub fn import_disk(disk: &Path, destination: &Path) -> Result<bool, SnapshotError> {
    let disk = fs::canonicalize(disk)?;
    if let Some(vars) = managed::vars_for_disk(&disk)? {
        create_snapshot(&disk, &vars, destination, "import")?;
        return Ok(true);
    }
    let _writer = writer_lease(&disk, "powered-off snapshot create")?;
    let _lease = MediaLease::acquire([disk.as_path()])?;
    // Check again while owning the disk; a restore may have published during discovery.
    if managed::vars_for_disk(&disk)?.is_some() {
        return Err(std::io::Error::other("media binding changed; retry import").into());
    }
    fs::create_dir(destination)?;
    let result = copy_and_sync(&disk, &destination.join("disk.raw"));
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result?;
    Ok(false)
}

#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod tests;
