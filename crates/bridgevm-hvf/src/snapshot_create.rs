//! Create a snapshot while preserving its logical and selected source media.

use super::*;

/// Capture `disk` and `vars` into `dest` as one atomic pair.
///
/// `vm_running` is passed in rather than probed here: only the caller knows
/// whether a helper still holds the media, and a snapshot of a running VM is
/// the failure this whole module exists to prevent.
pub fn create_snapshot(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    vm_running: bool,
    quota_bytes: u64,
) -> Result<SnapshotManifest, SnapshotError> {
    if vm_running {
        return Err(SnapshotError::VmRunning);
    }

    // Refuse before writing anything, not after filling the disk.
    let owner = managed::LockedPair::open(disk, vars)?;
    let (selected_disk, selected_vars) = owner.paths()?;
    let logical_disk = fs::canonicalize(disk)?;
    let logical_vars = fs::canonicalize(vars)?;
    let (disk, vars) = (selected_disk.as_path(), selected_vars.as_path());
    let projected = fs::metadata(disk)?.len() + fs::metadata(vars)?.len();
    if projected > quota_bytes {
        return Err(SnapshotError::QuotaExceeded {
            bytes: projected,
            quota: quota_bytes,
        });
    }

    let dest = prepare_destination(dest, [&logical_disk, &logical_vars, disk, vars])?;
    let staging = staging_path(&dest);
    // Refuse source overlap before cleaning debris from an interrupted attempt.
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let disk_bytes = copy_and_sync(disk, &staging.join(DISK_NAME))?;
    let vars_bytes = copy_and_sync(vars, &staging.join(VARS_NAME))?;

    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: vm_id.to_string(),
        disk_bytes,
        disk_sha256: sha256_file(&staging.join(DISK_NAME))?,
        vars_bytes,
        vars_sha256: sha256_file(&staging.join(VARS_NAME))?,
    };
    // The manifest is written last and is what makes the directory valid.
    write_file_atomically(&staging.join(MANIFEST_NAME), manifest.to_json().as_bytes())?;
    sync_dir(&staging)?;

    // Never remove the previous snapshot before its replacement is published.
    snapshot_publish::publish(&staging, &dest)?;
    Ok(manifest)
}

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
