//! Bounded guest directory snapshots; a refused snapshot never implies deletion, and only an
//! applied one ends a new session's wait (share_adopt.rs). One refused for its size pauses the
//! share (share_listing_pause.rs). An unchanged one comes as LSSAME (share_listing_same.rs).

use super::share_listing_report::refused_name_lines;
use super::*;
use crate::agent_console::harness_protocol::MAX_AGENT_LINE_BYTES;
use crate::agent_console::listing_same::{usable_gen, MAX_LISTING_GEN_BYTES};
#[path = "share_listing_refusal.rs"]
mod refusal;
use refusal::report_listing_limit;

// A listing at its byte bound, in base64 after `LSOK ` and with its gen, fits one agent line:
// one past the bound is refused for its length below, not discarded unread.
const _: () = assert!(
    "LSOK ".len() + share_sync::MAX_LISTING_BYTES.div_ceil(3) * 4 + 1 + MAX_LISTING_GEN_BYTES
        < MAX_AGENT_LINE_BYTES
);

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_listing_reply(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("LSOK ") {
            // `LSOK <b64> <gen>` from an agent asked with a gen (share_listing_same.rs).
            let (b64, gen) = rest.split_once(' ').map_or((rest, None), |(b64, gen)| (b64, Some(gen)));
            self.apply_share_listing(b64, gen);
            self.complete_in_flight();
        } else if let Some(gen) = line.strip_prefix("LSSAME ") {
            self.reapply_share_listing(gen);
            self.complete_in_flight();
        } else if line.starts_with("ERR") {
            if let Some(share) = self.share.as_mut() {
                share.listing_same.forget();
            }
            // A folder that is there but cannot be listed may hold newer Windows copies: the
            // waiting Mac files stay back. A missing one is created; its empty listing ends the wait.
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

    /// A reply past the line bound is discarded unread (harness_protocol.rs). The reply to a
    /// listing is then one too large: it pauses the share like any listing refused for its
    /// size, and the request ends instead of waiting for a reply that never comes.
    pub(in crate::agent_console) fn share_listing_line_too_long(&mut self) {
        if !matches!(self.in_flight, Some((ServiceReq::ShareLs, _))) {
            return;
        }
        println!("BVAGENT SHARE ls-error <listing-line-limit limit={MAX_AGENT_LINE_BYTES}>");
        if let Some(share) = self.share.as_mut() {
            share.listing_same.note_whole();
            let bytes = MAX_AGENT_LINE_BYTES / 4 * 3;
            share.note_listing_refused(share_sync::ListingLimitError::TooManyBytes { bytes });
        }
        self.complete_in_flight();
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

    fn apply_share_listing(&mut self, b64: &str, gen: Option<&str>) {
        if let Some(share) = self.share.as_mut() {
            share.listing_same.note_whole();
        }
        let max_encoded = share_sync::MAX_LISTING_BYTES.div_ceil(3) * 4;
        if b64.len() > max_encoded {
            println!(
                "BVAGENT SHARE ls-error <listing-encoded-limit bytes={} limit={max_encoded}>",
                b64.len()
            );
            if let Some(share) = self.share.as_mut() {
                let bytes = b64.len() / 4 * 3;
                share.note_listing_refused(share_sync::ListingLimitError::TooManyBytes { bytes });
            }
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
            let error = share_sync::ListingLimitError::TooManyBytes { bytes: bytes.len() };
            report_listing_limit(error);
            if let Some(share) = self.share.as_mut() {
                share.note_listing_refused(error);
            }
            return;
        }
        let gen = gen.filter(|gen| usable_gen(gen)).map(str::to_owned);
        if self.apply_listing_bytes(&bytes) {
            if let (Some(share), Some(gen)) = (self.share.as_mut(), gen) {
                share.listing_same.keep(gen, bytes);
            }
        }
    }

    /// LSSAME: the agent would send the kept listing's line again; it is applied again.
    fn reapply_share_listing(&mut self, gen: &str) {
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some((gen, bytes)) = share.listing_same.take(gen) else {
            println!("BVAGENT SHARE ls-error <listing-same-unknown>");
            return;
        };
        if self.apply_listing_bytes(&bytes) {
            if let Some(share) = self.share.as_mut() {
                share.listing_same.keep(gen, bytes);
            }
        }
    }

    /// Parse and apply a decoded listing within its byte bound; whether it was applied.
    fn apply_listing_bytes(&mut self, bytes: &[u8]) -> bool {
        let listing = match std::str::from_utf8(bytes) {
            Ok(listing) => listing,
            Err(_) => {
                println!("BVAGENT SHARE ls-error <listing-invalid-utf8>");
                return false;
            }
        };
        let puts = self.share_puts_under_way();
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        let refused = match share_sync::parse_ls_into(listing, &mut share.guest_ls_scratch) {
            Ok(refused) => refused,
            Err(error) => {
                report_listing_limit(error);
                share.note_listing_refused(error);
                return false;
            }
        };
        share.note_listing_applied();
        for line in refused_name_lines(&mut share.engine, &refused) {
            println!("{line}");
        }
        // A temporary's exact name (host_destination.rs) is neither fetched nor, absent, deleted;
        // nor is OS metadata (share_excluded.rs), folder or file.
        share.guest_ls_scratch.retain(|entry| {
            !share_sync::is_share_excluded(&entry.name)
                && (entry.is_dir || !host_destination::is_share_temporary(&entry.name))
        });
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
        true
    }
}
