//! Guest-initiated writes/deletes use pinned, no-follow directory descriptors.

use super::*;
use super::host_destination::Destination;

pub(in crate::agent_console) fn forget_absent_share_failures(share: &mut ShareState) {
    share.engine.forget_absent_failed(|name| Destination::absent(&share.host_dir, name));
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_delete(
        &mut self,
        name: &str,
        direction: ShareDelDirection,
        now: Instant,
    ) {
        if !matches!(direction, ShareDelDirection::GuestToHost) {
            return;
        }
        let Some(share) = self.share.as_mut() else { return; };
        match Destination::open(&share.host_dir, name, false).and_then(|d| d.delete()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                println!("BVAGENT SHARE del-refused {name} {e}");
                return;
            }
        }
        share.engine.on_host_deleted(name);
        println!("BVAGENT SHARE del guest->host {name} t={}", self.t_ms(now));
    }

    pub(in crate::agent_console) fn handle_share_get_end(&mut self, name: &str, _rest: &str, now: Instant) {
        let Some(finished) = self.take_finished_get() else { return; };
        if finished.bytes.len() != finished.total {
            println!("BVAGENT SHARE get-short {name} got={} expected={}",
                     finished.bytes.len(), finished.total);
            return;
        }
        let Some(share) = self.share.as_mut() else { return; };
        match share.engine.on_guest_file(name.to_string(), finished.bytes, None) {
            GuestFileOutcome::AlreadySynced => {}
            GuestFileOutcome::WriteHost(bytes) => {
                match Destination::open(&share.host_dir, name, true).and_then(|d| d.write(&bytes)) {
                    Ok(mtime) => {
                        share.engine.on_host_write_succeeded(name, mtime);
                        println!("BVAGENT SHARE guest->host {name} bytes={} t={}", bytes.len(), self.t_ms(now));
                    }
                    Err(e) => {
                        let exhausted = share.engine.on_host_write_failed(name);
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
    }
}
