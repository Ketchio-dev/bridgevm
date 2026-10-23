//! The share's size cap on guest files. A listing names each version with its
//! size, and an oversized one is skipped there, but the GET that fetches a
//! version returns whatever the file holds by then. A fetched file over the
//! cap is therefore refused too: it is not written, a host copy already synced
//! stays as it was, the skip is reported like an oversized listing, and the
//! listings that still name that version (same mtime) do not fetch it again,
//! so it is not retried on every poll. A new version is fetched as usual.

use super::*;

impl ShareSync {
    /// Refuse a fetched guest file of `size` bytes, over the cap, and note the
    /// listed version it was fetched for. The mtime comes as in `on_guest_file`.
    pub(super) fn refuse_over_cap(
        &mut self,
        name: String,
        size: u64,
        mtime: Option<&str>,
    ) -> GuestFileOutcome {
        let mtime = mtime
            .map(str::to_string)
            .or_else(|| self.pending_guest_mtime.remove(&name));
        if let Some(mtime) = mtime {
            self.fetched_over_cap.insert(name, mtime);
        }
        GuestFileOutcome::Skip(SkipReason::TooLarge { size })
    }

    /// Take the listed versions a GET found over the cap out of this listing's
    /// candidates, and forget refused versions the guest no longer lists.
    pub(super) fn drop_fetched_over_cap(&mut self) {
        let listed = &mut self.guest_file_entries_scratch;
        self.fetched_over_cap
            .retain(|name, mtime| listed.get(name).is_some_and(|entry| entry.mtime == *mtime));
        for name in self.fetched_over_cap.keys() {
            listed.remove(name);
        }
    }

    /// A skip is reported once per listed oversized version. Forget versions
    /// that left the listing, or guest listings grow this set without limit.
    pub(super) fn retain_guest_skips(&mut self) {
        let listed = &self.guest_file_entries_scratch;
        let max_bytes = self.max_bytes;
        self.guest_skip_seen.retain(|(name, mtime, _)| {
            listed
                .get(name)
                .is_some_and(|entry| entry.size > max_bytes && entry.mtime == *mtime)
        });
    }
}
