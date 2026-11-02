//! Resident GET completion is bound to both its active request and wire path.

use super::*;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_get_reply(
        &mut self,
        name: &str,
        line: &str,
        now: Instant,
    ) {
        if let Some(rest) = line.strip_prefix("GETEND ") {
            if self.handle_share_get_end(name, rest, now) {
                self.complete_in_flight();
            }
        } else if line.starts_with("ERR") {
            self.get_accum = None;
            println!("BVAGENT SHARE get-error {name} {line}");
            self.complete_in_flight();
        }
    }

    pub(in crate::agent_console) fn handle_share_get_end(
        &mut self,
        name: &str,
        _rest: &str,
        now: Instant,
    ) -> bool {
        if !matches!(
            self.in_flight.as_ref(),
            Some((ServiceReq::ShareGet { name: active }, _)) if active == name
        ) {
            return false;
        }
        let Some(share) = self.share.as_ref() else {
            return false;
        };
        write_share_guest_path_into(&share.guest_dir, name, &mut self.share_guest_path_scratch);
        if !self
            .get_accum
            .as_ref()
            .is_some_and(|accum| accum.path == self.share_guest_path_scratch)
        {
            return false;
        }
        let Some(finished) = self.take_finished_get() else {
            return false;
        };
        self.apply_finished_share_get(name, finished, now);
        true
    }

    fn apply_finished_share_get(&mut self, name: &str, finished: FinishedGet, now: Instant) {
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
        let bytes = match share
            .engine
            .on_guest_file(name.to_string(), finished.bytes, None)
        {
            GuestFileOutcome::AlreadySynced => return,
            GuestFileOutcome::Skip(reason) => return print_share_skip(name, reason),
            GuestFileOutcome::WriteHost(bytes) => bytes,
        };
        match Destination::open(&share.host_dir, name, true).and_then(|d| d.write(&bytes)) {
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
