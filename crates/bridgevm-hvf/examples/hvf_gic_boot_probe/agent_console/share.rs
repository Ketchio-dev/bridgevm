//! Host-share scanning, validation, and request construction.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShareFileReadError {
    Io,
    TooLarge(u64),
}

/// Short label for a service request, used in the stall-timeout print.
pub(super) fn req_kind(req: &ServiceReq) -> &'static str {
    match req {
        ServiceReq::ClipPoll => "clip-poll",
        ServiceReq::ClipPush(..) => "clip-push",
        ServiceReq::ClipCaps => "clip-caps",
        ServiceReq::ClipImagePush(_) => "clip-image-push",
        ServiceReq::ClipImagePull { .. } => "clip-image-pull",
        ServiceReq::ClipFilesPull { .. } => "clip-files-pull",
        ServiceReq::ClipFilesPush(_) => "clip-files-push",
        ServiceReq::ClipFilesAbort => "clip-files-abort",
        ServiceReq::Ctl(_) => "ctl",
        ServiceReq::Ping => "ping",
        ServiceReq::ShareLs => "share-ls",
        ServiceReq::ShareGet { .. } => "share-get",
        ServiceReq::ShareRange { .. } => "share-range",
        ServiceReq::ShareBigPut(_) => "share-big-put",
        ServiceReq::SharePut { .. } => "share-put",
        ServiceReq::ShareDel { .. } => "share-del",
        ServiceReq::TimeSync => "time-sync",
        ServiceReq::Drop(_) => "drop",
        ServiceReq::AgentUpdate(_) => "agent-update",
        ServiceReq::PrintRange => "print-range",
        ServiceReq::PrintDel { .. } => "print-del",
    }
}

pub(super) fn is_share_req(req: &ServiceReq) -> bool {
    matches!(
        req,
        ServiceReq::ShareLs
            | ServiceReq::ShareGet { .. }
            | ServiceReq::ShareRange { .. }
            | ServiceReq::ShareBigPut(_)
            | ServiceReq::SharePut { .. }
            | ServiceReq::ShareDel { .. }
    )
}

pub(super) fn init_share_from_env() -> Option<ShareState> {
    let spec = match std::env::var(SHARE_ENV) {
        Ok(value) if !value.is_empty() => value,
        _ => return None,
    };
    let Some((host, guest)) = parse_share_spec(&spec) else {
        println!("BVAGENT SHARE bad spec");
        return None;
    };
    let interval_ms = env_u64(SHARE_MS_ENV, DEFAULT_SHARE_MS).max(SHARE_MS_FLOOR);
    let max_kb = env_u64(SHARE_MAX_KB_ENV, DEFAULT_SHARE_MAX_KB);
    let mut share = ShareState::new(
        ShareSync::new(max_kb),
        PathBuf::from(host),
        guest,
        Duration::from_millis(interval_ms),
    );
    share.host_watch = HostWatch::Pending;
    // What synced in earlier sessions, kept beside the VM's own UEFI vars (share_sync_file.rs).
    if let Some(vars) = std::env::var_os("BRIDGEVM_AARCH64_UEFI_VARS").filter(|vars| !vars.is_empty()) {
        share.attach_sync_state(Path::new(&vars));
    }
    // Until a READY says the agent moves files in ranges (share_big.rs).
    share.engine.set_whole_file_limit(Some(harness_protocol::MAX_AGENT_GET_BYTES as u64));
    Some(share)
}

pub(super) fn parse_share_spec(spec: &str) -> Option<(String, String)> {
    let (host, guest) = spec.split_once("::")?;
    if host.is_empty() || guest.is_empty() {
        return None;
    }
    Some((host.to_string(), guest.to_string()))
}

#[path = "share/share_host_scan.rs"]
mod share_host_scan;
pub(super) use share_host_scan::*;
#[path = "share/share_big.rs"]
mod share_big;
pub(super) use share_big::{is_out_of_space, parse_range_beg, BigTransfer, SHARE_FREE_MARGIN, SHARE_RANGE_BYTES};
#[path = "share/share_big_put.rs"]
mod share_big_put;
pub(super) use share_big_put::{start_big_put, BigPutStart, BigPutStep};
#[path = "share/share_stage_writer.rs"]
mod share_stage_writer;

pub(super) fn host_rel_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for component in rel.components() {
        let component = component.as_os_str().to_string_lossy();
        // A literal backslash is legal in a macOS filename but is a path
        // separator in the Windows guest. Treating it as a normal byte here
        // creates two different sync keys and repeats PUT/tombstone actions.
        if component.contains('\\') {
            return None;
        }
        // A stand-in character in a Mac name would come back from the guest
        // as the character it stands for, renaming the Mac file
        // (share_guest_names.rs); such a name is left out of the share.
        if component.chars().any(share_sync::is_stand_in) {
            return None;
        }
        if !out.is_empty() {
            out.push('/');
        }
        // Keys are composed; the file is found under either form (share_guest_names.rs).
        out.push_str(&share_sync::composed(&component));
    }
    Some(out)
}

pub(super) fn file_mtime_ms(path: &Path) -> Option<u128> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| file_mtime_ms_from_meta(&meta))
}

pub(super) fn file_mtime_ms_from_meta(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis())
}

// Shared-folder skip diagnostics.

pub(in crate::agent_console) fn print_host_skip_once(
    share: &mut ShareState,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    let limit = share.engine.max_bytes();
    print_host_skip_once_seen(&mut share.host_skip_seen, name, mtime_ms, kind, size, limit);
}

pub(in crate::agent_console) fn print_host_skip_once_seen(
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
    limit: u64,
) {
    if !host_skip_seen.insert((name.to_string(), mtime_ms, kind)) {
        return;
    }
    match kind {
        HostSkipKind::TooLarge => println!("{}", too_large_line(name, size, limit, "mac")),
    }
}

/// The app's notice names the limit that applied and the side the file is on
/// (HvfShareTooLarge in HvfShareStop.swift): 64 MiB until the agent moves files in ranges.
pub(in crate::agent_console) fn too_large_line(name: &str, size: u64, limit: u64, side: &str) -> String {
    format!("BVAGENT SHARE skip {name} too-large {size} limit={limit} {side}")
}

pub(in crate::agent_console) fn print_share_skip(name: &str, reason: SkipReason) {
    match reason {
        SkipReason::TooLarge { size, limit } => println!("{}", too_large_line(name, size, limit, "windows")),
        SkipReason::WindowsAlias(names) => println!("BVAGENT SHARE skip {name} alias {names:?}"),
    }
}

#[path = "share/share_listing_reply.rs"]
mod share_listing_reply;
#[path = "share/share_listing_reply_tests.rs"]
mod share_listing_reply_tests;
#[path = "share/share_skip_line_tests.rs"]
mod share_skip_line_tests;

/// The guest path for a share key. Where the guest listed the key, or the
/// folder it is in, under a spelling other than the key's own guest form (a
/// decomposed Windows name), that spelling is used, so a new file goes into
/// the existing folder rather than a composed twin of it.
pub(super) fn write_share_guest_path_into(share: &ShareState, name: &str, out: &mut String) {
    out.clear();
    out.reserve(share.guest_dir.len() + 1 + name.len());
    out.push_str(&share.guest_dir);
    out.push('\\');
    let mut end = name.len();
    while !share.guest_names.is_empty() {
        if let Some(listed) = share.guest_names.get(&share_sync::windows_key(&name[..end])) {
            share_sync::append_guest_rel_into(listed, out);
            if end < name.len() {
                out.push('\\');
                share_sync::append_guest_rel_into(&name[end..], out);
            }
            return;
        }
        let Some(slash) = name[..end].rfind('/') else { break };
        end = slash;
    }
    share_sync::append_guest_rel_into(name, out);
}

/// After a PUT's Mac time: ` <size> <time>` of the guest copy last synced, when known. The agent
/// keeps a different file there as "(Windows copy)" before replacing it; older agents ignore it.
pub(super) fn write_put_guest_version(share: &ShareState, name: &str, out: &mut String) {
    if let Some((size, stamp)) = share.engine.synced_guest_stat(name) {
        use std::fmt::Write as _;
        let _ = write!(out, " {size} {stamp}");
    }
}

impl AgentConsoleHarness {
    /// Recheck the selected root before a host deletion becomes a guest DEL.
    pub(in crate::agent_console) fn write_share_delete_wire(&mut self, name: &str) -> bool {
        let Some(share) = self.share.as_mut() else {
            println!("BVAGENT SHARE del-skipped {name} share-disabled");
            return false;
        };
        if !share.root_matches() {
            println!("BVAGENT SHARE del-skipped {name} root-unavailable");
            return false;
        }
        if std::fs::symlink_metadata(share.host_dir.join(name)).is_ok() {
            println!("BVAGENT SHARE del-skipped {name} mac-copy-returned");
            return false;
        }
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        self.service_line_scratch.push_str("DEL ");
        base64_encode_into(
            self.share_guest_path_scratch.as_bytes(),
            &mut self.service_line_scratch,
        );
        if self.delete_if_unchanged {
            if let Some((size, stamp)) = share.engine.synced_guest_stat(name) {
                use std::fmt::Write as _;
                let _ = write!(self.service_line_scratch, " {size} {stamp}");
            } else {
                println!("BVAGENT SHARE del-unchecked {name} guest-version-unknown");
            }
        }
        self.service_line_scratch.push('\n');
        true
    }
}

// Guest-write refusals that hold until the guest file changes. The guest write
// policy refuses a write whose temporary keeps execute bits, or whose
// quarantine mark the share will not hold: ENOTSUP, EACCES or EPERM (a deny
// ACL, a flag), or an msdos/exfat mark that does not read back. Fetching the
// same unchanged guest file again cannot succeed, so the share remembers these
// refusals (share_write_failures.rs). Any other failure is retried at the next
// guest listing.

/// The reason a write was refused for good.
#[derive(Debug)]
struct Persistent(String);

impl std::fmt::Display for Persistent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Persistent {}

/// A refusal that holds until the guest file changes, shown as `reason`.
fn persistent(kind: std::io::ErrorKind, reason: impl Into<String>) -> std::io::Error {
    std::io::Error::new(kind, Persistent(reason.into()))
}

/// Whether `error` is such a refusal; any other error is retried.
fn is_persistent(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<Persistent>())
}

#[path = "share/harness_get_end.rs"]
mod harness_get_end;
#[path = "share/host_destination.rs"]
mod host_destination;
pub(super) use host_destination::{free_bytes, mark_guest_item, NEW_DIR_MODE, NEW_FILE_MODE};
pub(super) use host_destination::snapshot::{version as source_version, SourceVersion};
#[path = "share/share_host_snapshot.rs"]
mod share_host_snapshot;
pub(super) use share_host_snapshot::*;
#[path = "share/share_host_change.rs"]
mod share_host_change;
#[path = "share/share_host_write.rs"]
mod share_host_write;
#[path = "share/share_put.rs"]
mod share_put;
pub(super) use share_put::*;

#[cfg(test)]
mod failure_tests {
    //! Failure-budget exhaustion stops share work, not unrelated console service.

    use super::*;
    use crate::agent_console::agent_console_tests::tests::harness;
    use crate::agent_console::share::share_host_write::get_test_fixture::fetched;

    /// An owned share root for the destination-failure budget boundary.
    struct OwnedRoot(PathBuf);

    impl OwnedRoot {
        fn new() -> Self {
            for nonce in 0..128 {
                let path = std::env::temp_dir().join(format!(
                    "bvagent-share-failure-{}-{nonce}",
                    std::process::id()
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create owned share root: {error}"),
                }
            }
            panic!("no fresh owned share root available");
        }
    }

    impl Drop for OwnedRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn nearly_exhausted_share() -> (AgentConsoleHarness, OwnedRoot) {
        let root = OwnedRoot::new();
        let mut h = harness();
        let mut engine = ShareSync::new(1024);
        for i in 0..4095 {
            let name = format!("blocked/{i}");
            engine.on_guest_file(name.clone(), vec![1], Some("g1"));
            assert!(!engine.on_host_write_failed(&name));
        }
        h.share = Some(ShareState::new(
            engine,
            root.0.clone(),
            "C:\\share".into(),
            Duration::from_secs(1),
        ));
        (h, root)
    }

    #[test]
    fn failed_destination_limit_disables_share_and_drops_queued_puts_and_deletes() {
        let (mut h, _root) = nearly_exhausted_share();
        h.queue.push_back(ServiceReq::ShareLs);
        h.queue.push_back(share_put_req("a".into(), vec![1], 1, None));
        h.queue.push_back(ServiceReq::ShareDel { name: "a".into(), direction: ShareDelDirection::HostToGuest });
        h.queue.push_back(ServiceReq::Ping);
        // A real root verifies, then the invalid component exhausts the same budget.
        fetched(&mut h, "../refused", &[1]);
        assert!(h.share.is_none());
        assert_eq!(h.queue.len(), 1);
        assert!(matches!(h.queue.front(), Some(ServiceReq::Ping)));
    }
}
