//! An idle pass. Every 2 s poll planned a 100,000-file share from scratch twice: the Mac files
//! (on_host_scan, about 93 ms) and the guest listing (on_guest_listing, with its parse about
//! 100 ms), about 110 ms/s of the probe's time for a folder in which nothing changed.
//!
//! A pass is idle when it planned nothing, changed nothing a later pass reads (each change it
//! or its setup makes sets `pass_changed`), and nothing is pending: no transfer half done or
//! held (pending names, a preferred guest edit, a refused or unconfirmed PUT, a PUT under way, a
//! failed write that is retried, a PUT stamp not seen yet), no Windows-alias hold, and the
//! session's adoption over (two listings applied, share_adopt.rs). A pass is a function of the
//! engine and its input, so the same pass on the same input, with the engine untouched since,
//! plans and changes nothing again: the service skips it (agent_console/share/share_idle.rs),
//! and a debug build runs it anyway on a copy to check that.
//!
//! Marks and synced versions are not read by a pass after the first listing, so their expiry
//! does not count as a change.

use super::*;

impl FileRecord {
    /// A retry or a confirmation still to come.
    pub(super) fn flagged(&self) -> bool {
        self.host_write_failed || self.put_unconfirmed || self.awaiting_guest_stamp
    }
}

impl ShareSync {
    /// The last pass was idle.
    pub fn idle(&self) -> bool {
        self.idle
    }

    pub(super) fn end_pass(&mut self, actions: &[SyncAction], flagged: bool) {
        let changed = std::mem::take(&mut self.pass_changed);
        self.idle = actions.is_empty() && !changed && !flagged && self.nothing_pending();
    }

    fn nothing_pending(&self) -> bool {
        self.listed
            && self.listings >= 2
            && self.pending_guest_mtime.is_empty()
            && self.pending_host_changed.is_empty()
            && self.guest_changed.is_empty()
            && self.put_rejected.is_empty()
            && self.puts_under_way.is_empty()
            && self.host_aliased.is_empty()
    }

    /// An idle guest listing applied again without its pass: it still counts as a listing.
    pub fn note_unchanged_listing(&mut self) {
        debug_assert!(self.idle && self.listings >= 2);
        self.listings = self.listings.saturating_add(1);
    }

    /// Debug check of a skipped pass: `rerun`, a copy of this engine that ran it, is this
    /// engine, with one more listing for a guest pass.
    pub fn same_after_rerun(&self, rerun: &ShareSync, listing: bool) -> bool {
        let mut expect = self.clone();
        if listing {
            expect.listings = expect.listings.saturating_add(1);
        }
        expect.idle = rerun.idle;
        expect == *rerun
    }
}
