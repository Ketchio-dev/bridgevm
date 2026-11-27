//! A replaced or unavailable root is not a snapshot of host deletions.
//! Identity checks bracket the path-based walk; concurrent swaps inside that
//! interval and races below the selected root are not an atomic snapshot.

use super::*;

/// A scan of the Mac folder for the host pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agent_console) enum HostScan {
    /// The folder was walked.
    Walked,
    /// The kept walk is planned from again.
    Kept,
    /// The kept walk's last plan was idle and stands (share_idle.rs): no pass.
    Unchanged,
}

/// None when the bound root is unavailable, replaced, or incompletely listed.
pub(in crate::agent_console) fn scan_share_host_dir(share: &mut ShareState, now: Instant) -> Option<HostScan> {
    if !share.root_matches() {
        return None;
    }
    // Nothing below the root changed since the last complete walk (share_host_watch.rs).
    if let Some(walk) = share.kept_host_walk(now) {
        if share.host_plan_unchanged(walk) {
            share.check_skipped_host_pass(&|name: &str| host_destination::Destination::absent(&share.host_dir, name));
            share.report_root_state(false);
            return Some(HostScan::Unchanged);
        }
        let unseen = share.reuse_host_walk();
        return accept_scan(share, true, unseen, now).then_some(HostScan::Kept);
    }
    #[cfg(test)]
    {
        share.host_scans += 1;
    }
    let changes = share.begin_host_walk();
    share.host_actions_scanned = std::time::SystemTime::now();
    share.host_scan_work.clear();
    let mut unseen = Vec::new();
    let listed = scan_share_host_dir_inner(
        &share.host_dir,
        &share.host_dir,
        share.engine.max_bytes(),
        &mut share.host_skip_seen,
        &mut share.host_scan_work,
        &mut unseen,
    );
    if listed {
        share.keep_host_walk(changes, &unseen, now);
    }
    let accepted = accept_scan(share, listed, unseen, now);
    if !accepted {
        share.forget_host_walk();
    }
    accepted.then_some(HostScan::Walked)
}

/// Publish an accepted scan, then announce recovery from a root deferral.
fn accept_scan(share: &mut ShareState, listed: bool, unseen: Vec<String>, now: Instant) -> bool {
    if !listed || !share.root_matches() {
        share.report_root_state(true);
        return false;
    }
    // Setup of the host pass, which notes its own changes (share_idle.rs).
    share_host_write::forget_absent_share_failures(share);
    std::mem::swap(&mut share.host_scan_scratch, &mut share.host_scan_work);
    share.engine.for_pass().set_host_unseen(unseen);
    share.engine.for_pass().retry_refused_puts(now);
    share.report_root_state(false);
    true
}

/// Recursive host walk; snapshots are admitted only by the caller. False when
/// `dir` cannot be listed completely.
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
        // OS metadata (share_excluded.rs) is not shared: no stat, nothing below it walked.
        if share_sync::is_share_excluded(&name) {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path);
        // A temporary, the host's or the agent's (host_destination.rs), is never shared; the
        // host's own is removed once the process that wrote it is gone.
        if meta.as_ref().is_ok_and(|meta| meta.is_file()) && host_destination::is_share_temporary(&name) {
            remove_stale_temporary(&path, &entry.file_name());
            continue;
        }
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
                max_bytes,
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

/// `.bridgevm-sync-<pid>-<n>` of a process that no longer runs: an interrupted copy.
fn remove_stale_temporary(path: &Path, name: &std::ffi::OsStr) {
    let Some(pid) = name.to_str().and_then(host_destination::host_temporary_pid) else {
        return;
    };
    // SAFETY: signal 0 only checks that the process exists.
    let gone = unsafe { libc::kill(pid, 0) } < 0
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
    let regular = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file());
    if gone && regular && std::fs::remove_file(path).is_ok() {
        println!("BVAGENT SHARE removed-stale-temporary");
    }
}
