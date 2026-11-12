//! Guest file adoption starts only after the selected destination root is verified.

use super::*;

#[cfg(test)]
#[path = "share_get_mtime_tests.rs"]
mod mtime_tests;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn apply_finished_share_get(
        &mut self,
        name: &str,
        finished: FinishedGet,
        now: Instant,
    ) {
        if finished.bytes.len() != finished.total {
            println!(
                "BVAGENT SHARE get-short {name} got={} expected={}",
                finished.bytes.len(),
                finished.total
            );
            return;
        }
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let root = match share.checked_root() {
            Ok(root) => root,
            Err(_) => {
                println!("BVAGENT SHARE get-deferred <root-unavailable>");
                return;
            }
        };
        // A preferred guest edit goes only over the Mac file it was preferred to (share_synced.rs).
        let recovery = share.engine.recovery_copy(name);
        let empty_recovery = recovery && finished.bytes.is_empty();
        let mac = if recovery { Some(None) } else { share.fetch_mac.get(name).copied().or_else(|| share.engine.preferred_mac_stat(name).map(Some)) };
        let bytes = match share
            .engine
            .on_guest_file(name.to_string(), finished.bytes, None)
        {
            GuestFileOutcome::AlreadySynced => {
                share.fetch_mac.remove(name);
                return;
            }
            GuestFileOutcome::Skip(reason) => return print_share_skip(name, reason),
            GuestFileOutcome::WriteHost(bytes) => bytes,
        };
        if empty_recovery {
            share.fetch_mac.remove(name);
            share.engine.on_host_copy_kept(name);
            return;
        }
        let mtime = (share.engine.guest_mtime(name))
            .and_then(share_sync::share_mtime::guest_mtime_ms)
            .and_then(share_sync::share_mtime::system_time);
        share.forget_host_walk();
        // The guest's copy may not be on its disk yet: marked first (share_synced.rs).
        share.mark_fetched(name, now);
        let written = Destination::from_root(root, name, true)
            .and_then(|d| d.write_at_unless_changed(&bytes, mtime, mac));
        match written {
            Ok(mtime) => {
                share.fetch_mac.remove(name);
                share.engine.on_host_write_succeeded(name, mtime);
                println!(
                    "BVAGENT SHARE guest->host {name} bytes={} t={}",
                    bytes.len(),
                    self.t_ms(now)
                );
            }
            Err(e) if crate::agent_console::share::host_destination::is_mac_changed(&e) => {
                share.fetch_mac.remove(name);
                share.engine.on_host_copy_kept(name);
                println!("BVAGENT SHARE skip {name} mac-changed");
            }
            Err(e) => {
                let exhausted = share.engine.on_host_write_error(name, is_persistent(&e));
                // A full Mac disk fails these writes; run.log may be on it (host_log).
                bridgevm_hvf::host_log::out(format_args!("BVAGENT SHARE write-refused {name} {e}"));
                if crate::agent_console::share::share_big::is_out_of_space(&e) {
                    share.report_low_space(name, bytes.len() as u64);
                }
                if exhausted {
                    self.share = None;
                    self.queue.retain(|request| !is_share_req(request));
                    bridgevm_hvf::host_log::out("BVAGENT SHARE disabled failed-destination-limit");
                }
            }
        }
    }
}
