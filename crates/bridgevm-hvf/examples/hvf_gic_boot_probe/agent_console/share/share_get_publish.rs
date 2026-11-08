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
        let bytes = match share
            .engine
            .on_guest_file(name.to_string(), finished.bytes, None)
        {
            GuestFileOutcome::AlreadySynced => return,
            GuestFileOutcome::Skip(reason) => return print_share_skip(name, reason),
            GuestFileOutcome::WriteHost(bytes) => bytes,
        };
        let mtime = (share.engine.guest_mtime(name))
            .and_then(share_sync::share_mtime::guest_mtime_ms)
            .and_then(share_sync::share_mtime::system_time);
        share.forget_host_walk();
        match Destination::from_root(root, name, true).and_then(|d| d.write_at(&bytes, mtime)) {
            Ok(mtime) => {
                share.engine.on_host_write_succeeded(name, mtime);
                println!(
                    "BVAGENT SHARE guest->host {name} bytes={} t={}",
                    bytes.len(),
                    self.t_ms(now)
                );
            }
            Err(e) => {
                let exhausted = share.engine.on_host_write_error(name, is_persistent(&e));
                println!("BVAGENT SHARE write-refused {name} {e}");
                if exhausted {
                    self.share = None;
                    self.queue.retain(|request| !is_share_req(request));
                    println!("BVAGENT SHARE disabled failed-destination-limit");
                }
            }
        }
    }
}
