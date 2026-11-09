//! Bounded guest directory snapshots; a refused snapshot never implies deletion.

use super::share_listing_report::refused_name_lines;
use super::*;
#[path = "share_listing_refusal.rs"]
mod refusal;
use refusal::report_listing_limit;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_listing_reply(&mut self, line: &str) {
        // Whatever the listing holds, a new session stops holding Mac files back.
        if line.starts_with("LSOK ") || line.starts_with("ERR") {
            if let Some(share) = self.share.as_mut() {
                share.engine.end_first_listing_wait();
            }
        }
        if let Some(b64) = line.strip_prefix("LSOK ") {
            self.apply_share_listing(b64);
            self.complete_in_flight();
        } else if line.starts_with("ERR") {
            println!("BVAGENT SHARE ls-error {line}");
            self.complete_in_flight();
            self.create_guest_share_root_once();
        } else if parse_out_line(line).is_some_and(|(exit, _)| exit == 255) {
            if !self.share_old_agent_warned {
                println!("BVAGENT SHARE agent-too-old (needs v3-share2); share disabled");
                self.share_old_agent_warned = true;
            }
            self.share = None;
            self.queue.retain(|req| !is_share_req(req));
            self.complete_in_flight();
        }
    }

    /// Only a PUT made the guest folder, so with an empty Mac folder Windows
    /// never got one. Until a file has synced an empty listing deletes
    /// nothing, so a failed listing may create the folder; never later, when
    /// a folder the user deleted would come back empty and empty the Mac one.
    fn create_guest_share_root_once(&mut self) {
        let Some(share) = self.share.as_mut() else {
            return;
        };
        if share.guest_root_create_sent || share.engine.has_synced() {
            return;
        }
        share.guest_root_create_sent = true;
        let mut path = String::new();
        base64_encode_into(share.guest_dir.as_bytes(), &mut path);
        let script = format!(
            "[void][IO.Directory]::CreateDirectory([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{path}')))"
        );
        println!("BVAGENT SHARE create-guest-folder");
        self.queue
            .push_back(ServiceReq::Ctl(crate::agent_console::desktop_switch::powershell_command(&script)));
    }

    /// The names of the PUTs the service holds now: held for their marks, queued, in flight,
    /// or the large one under way (ShareSync::set_puts_under_way).
    fn share_puts_under_way(&self) -> Vec<String> {
        let Some(share) = self.share.as_ref() else {
            return Vec::new();
        };
        let put = |req: &ServiceReq| match req {
            ServiceReq::SharePut { name, .. } => Some(name.clone()),
            _ => None,
        };
        let mut names: Vec<String> = share.held_put.iter().filter_map(put).collect();
        names.extend(self.queue.iter().filter_map(put));
        names.extend(self.in_flight.iter().filter_map(|(req, _)| put(req)));
        if let Some(BigTransfer::Put(big)) = share.big.as_ref() {
            names.push(big.name.clone());
        }
        names
    }

    fn apply_share_listing(&mut self, b64: &str) {
        let max_encoded = share_sync::MAX_LISTING_BYTES.div_ceil(3) * 4;
        if b64.len() > max_encoded {
            println!(
                "BVAGENT SHARE ls-error <listing-encoded-limit bytes={} limit={max_encoded}>",
                b64.len()
            );
            return;
        }
        let bytes = match base64_decode(b64) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("BVAGENT SHARE ls-error <base64 decode error: {error:?}>");
                return;
            }
        };
        if bytes.len() > share_sync::MAX_LISTING_BYTES {
            report_listing_limit(share_sync::ListingLimitError::TooManyBytes {
                bytes: bytes.len(),
            });
            return;
        }
        let listing = match std::str::from_utf8(&bytes) {
            Ok(listing) => listing,
            Err(_) => {
                println!("BVAGENT SHARE ls-error <listing-invalid-utf8>");
                return;
            }
        };
        let puts = self.share_puts_under_way();
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let refused = match share_sync::parse_ls_into(listing, &mut share.guest_ls_scratch) {
            Ok(refused) => refused,
            Err(error) => {
                report_listing_limit(error);
                return;
            }
        };
        for line in refused_name_lines(&mut share.engine, &refused) {
            println!("{line}");
        }
        // A temporary's exact name (host_destination.rs) is neither fetched nor, absent, deleted.
        share
            .guest_ls_scratch
            .retain(|entry| entry.is_dir || !host_destination::is_share_temporary(&entry.name));
        share.key_guest_listing();
        share.engine.set_puts_under_way(puts);
        let actions = share
            .engine
            .on_guest_listing_normalized(share.guest_ls_scratch.drain(..));
        for action in actions {
            match action {
                SyncAction::Get { name } => self.queue.push_back(ServiceReq::ShareGet { name }),
                SyncAction::DeleteHost { name } => self.queue.push_back(ServiceReq::ShareDel {
                    name,
                    direction: ShareDelDirection::GuestToHost,
                }),
                SyncAction::DeleteGuest { name } => self.queue.push_back(ServiceReq::ShareDel {
                    name,
                    direction: ShareDelDirection::HostToGuest,
                }),
                SyncAction::Skip { name, reason } => print_share_skip(&name, reason),
            }
        }
    }
}
