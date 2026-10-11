//! Host-share scan. A name that exists but is not a readable file within the
//! cap (oversized, a symlink or special file, an unlistable directory) is
//! reported as unseen rather than dropped, so it never reads as deleted.
//! Not covered: an empty folder put in place of the root lists as every file
//! deleted, exactly like the user emptying the share.

use super::*;

/// False when the share root itself cannot be listed completely.
pub(in crate::agent_console) fn scan_share_host_dir(share: &mut ShareState) -> bool {
    share_host_write::forget_absent_share_failures(share);
    share.host_scan_scratch.clear();
    let mut unseen = Vec::new();
    let listed = scan_share_host_dir_inner(
        &share.host_dir,
        &share.host_dir,
        share.engine.max_bytes(),
        &mut share.host_skip_seen,
        &mut share.host_scan_scratch,
        &mut unseen,
    );
    share.engine.set_host_unseen(unseen);
    listed
}

/// False when `dir` cannot be listed completely.
fn scan_share_host_dir_inner(
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
