//! Recursive host walk; snapshots are admitted only by the caller.

use super::*;

/// False when `dir` cannot be listed completely.
pub(super) fn scan_share_host_dir_inner(
    root: &Path,
    dir: &Path,
    max_bytes: u64,
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    files: &mut Vec<HostFile>,
    unseen: &mut Vec<String>,
) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut complete = true;
    for entry in entries {
        // A failed read may have hidden any name in this directory.
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        let path = entry.path();
        let Some(name) = host_rel_path(root, &path) else {
            continue;
        };
        let meta = std::fs::symlink_metadata(&path);
        if meta.as_ref().is_ok_and(|meta| meta.is_dir()) {
            if !scan_share_host_dir_inner(root, &path, max_bytes, host_skip_seen, files, unseen) {
                unseen.push(name);
            }
            continue;
        }
        let Some(meta) = meta.ok().filter(|meta| meta.is_file()) else {
            unseen.push(name);
            continue;
        };
        let mtime_ms = file_mtime_ms_from_meta(&meta).unwrap_or(0);
        let size = meta.len();
        if size > max_bytes {
            print_host_skip_once_seen(
                host_skip_seen,
                &name,
                mtime_ms,
                HostSkipKind::TooLarge,
                size,
            );
            unseen.push(name);
            continue;
        }
        files.push(HostFile {
            name,
            size,
            mtime_ms,
        });
    }
    complete
}
