//! Shared-folder skip diagnostics.

use super::*;

pub(in crate::agent_console) fn print_host_skip_once(
    share: &mut ShareState,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    print_host_skip_once_seen(&mut share.host_skip_seen, name, mtime_ms, kind, size);
}

pub(in crate::agent_console) fn print_host_skip_once_seen(
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    if !host_skip_seen.insert((name.to_string(), mtime_ms, kind)) {
        return;
    }
    match kind {
        HostSkipKind::TooLarge => println!("BVAGENT SHARE skip {name} too-large {size}"),
    }
}

pub(in crate::agent_console) fn print_guest_skip(name: &str, reason: SkipReason) {
    match reason {
        SkipReason::TooLarge { size } => {
            println!("BVAGENT SHARE skip {name} too-large {size}")
        }
    }
}
