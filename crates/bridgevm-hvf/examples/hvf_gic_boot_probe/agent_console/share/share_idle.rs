//! An idle share is not planned again (share_sync_idle.rs). The last idle host pass and the
//! last idle guest pass are noted with their input, the kept walk (share_host_watch.rs) or the
//! kept listing (share_listing_same.rs), and the engine's touch count after them. A poll with
//! the same input that finds the engine untouched since skips that pass: a quiet FSEvents
//! stream and an LSSAME then cost a 100,000-file share a request, not about 190 ms. Anything
//! else that changes the engine (a transfer, a READY, an adoption, a mark) is a touch, and so
//! is a pass that was not idle: the next poll plans again.

use super::*;
use std::ops::{Deref, DerefMut};

/// The share engine. Lent out to change it, it counts a touch; a pass and its setup, which
/// note their own changes (ShareSync::idle), use `for_pass`.
pub(in crate::agent_console) struct Engine {
    sync: ShareSync,
    touched: u64,
}

impl From<ShareSync> for Engine {
    fn from(sync: ShareSync) -> Self {
        Self { sync, touched: 0 }
    }
}

impl Deref for Engine {
    type Target = ShareSync;
    fn deref(&self) -> &ShareSync {
        &self.sync
    }
}

impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut ShareSync {
        self.touched += 1;
        &mut self.sync
    }
}

impl Engine {
    /// For a pass and its setup, and for what no pass reads after the first listing (marks,
    /// synced versions, refused-name reports).
    pub(in crate::agent_console) fn for_pass(&mut self) -> &mut ShareSync {
        &mut self.sync
    }
}

/// The input of the last idle pass of each kind, and the touch count after it.
#[derive(Default)]
pub(in crate::agent_console) struct IdlePasses {
    host: Option<(u64, u64)>,
    guest: Option<(u64, u64)>,
    inputs: u64,
}

impl ShareState {
    /// A serial for a kept walk or listing.
    pub(in crate::agent_console) fn new_idle_input(&mut self) -> u64 {
        self.idle.inputs += 1;
        self.idle.inputs
    }

    /// The last host pass planned from the kept walk `walk`, was idle, and nothing touched the
    /// engine since: planning from it again would plan nothing.
    pub(in crate::agent_console) fn host_plan_unchanged(&self, walk: u64) -> bool {
        self.idle.host == Some((walk, self.engine.touched))
    }

    /// The same for the guest pass over the kept listing `listing`.
    pub(in crate::agent_console) fn guest_plan_unchanged(&self, listing: u64) -> bool {
        self.idle.guest == Some((listing, self.engine.touched))
    }

    /// The host pass over the scan in `host_scan_scratch`.
    pub(in crate::agent_console) fn plan_host_scan(&mut self) -> Vec<SyncAction> {
        #[cfg(test)]
        {
            self.host_passes += 1;
        }
        let actions = (self.engine.for_pass()).on_host_scan_normalized(self.host_scan_scratch.drain(..));
        let walk = self.host_walk_serial();
        self.idle.host = self.settle(walk);
        actions
    }

    /// After a guest pass over a listing, kept as `listing` when it has a gen.
    pub(in crate::agent_console) fn note_guest_pass(&mut self, listing: Option<u64>) {
        #[cfg(test)]
        {
            self.guest_passes += 1;
        }
        self.idle.guest = self.settle(listing);
    }

    fn settle(&mut self, input: Option<u64>) -> Option<(u64, u64)> {
        if !self.engine.idle() {
            self.engine.touched += 1;
            return None;
        }
        input.map(|input| (input, self.engine.touched))
    }

    /// Debug builds run a skipped host pass anyway, as the scan would have, on a copy: it must
    /// plan and change nothing.
    pub(in crate::agent_console) fn check_skipped_host_pass(&self, is_absent: &dyn Fn(&str) -> bool) {
        if !cfg!(debug_assertions) {
            return;
        }
        let (files, unseen) = self.kept_host_walk_files().expect("a skipped host pass has a kept walk");
        let mut rerun = self.engine.sync.clone();
        rerun.forget_absent_failed(is_absent);
        rerun.set_host_unseen(unseen.iter().cloned());
        let actions = rerun.on_host_scan_normalized(files.iter().cloned());
        assert!(actions.is_empty() && rerun.idle(), "a skipped host pass plans {actions:?}");
        assert!(self.engine.sync.same_after_rerun(&rerun, false), "a skipped host pass changes the engine");
    }

    /// The same for a skipped guest pass over `entries`.
    pub(in crate::agent_console) fn check_skipped_guest_pass(&self, entries: &[LsEntry]) {
        if !cfg!(debug_assertions) {
            return;
        }
        let mut rerun = self.engine.sync.clone();
        rerun.set_puts_under_way(Vec::new());
        let actions = rerun.on_guest_listing_normalized(entries.iter().cloned());
        assert!(actions.is_empty() && rerun.idle(), "a skipped guest pass plans {actions:?}");
        assert!(self.engine.sync.same_after_rerun(&rerun, true), "a skipped guest pass changes the engine");
    }
}
