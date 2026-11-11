//! A guest listing refused for its size pauses the share. Live (share-bigloss-20261111, a
//! folder of 4200 files): every listing was refused, so a Windows edit never reached the Mac,
//! and the next session sent every Mac file over it. Without a listing the host cannot tell a
//! newer Windows edit, so while the latest listing is refused for its size no PUT and no guest
//! deletion goes out (share_host_change.rs), and nothing comes from Windows, which only a
//! listing plans. A new session's Mac files keep waiting for its first applied listing
//! (share_adopt.rs). Once a listing is applied (the folder got smaller) the share goes on as
//! before. The line printed at each change is the app's notice (HvfShareStop.swift).

use super::*;
use crate::agent_console::share_sync::{self, ListingLimitError};

impl ShareState {
    /// The latest guest listing was refused for its size.
    pub(in crate::agent_console) fn listing_too_large(&self) -> bool {
        self.listing_too_large
    }

    /// A guest listing refused for its bounds; one refused for its size pauses the share.
    pub(in crate::agent_console) fn note_listing_refused(&mut self, error: ListingLimitError) {
        let (reason, limit) = match error {
            ListingLimitError::TooManyEntries { .. } => {
                ("too-many-entries", share_sync::MAX_LISTING_ENTRIES)
            }
            ListingLimitError::TooManyBytes { .. } => {
                ("too-many-bytes", share_sync::MAX_LISTING_BYTES)
            }
            ListingLimitError::InvalidRow { .. } => return,
        };
        if !self.listing_too_large {
            self.listing_too_large = true;
            print_pause_line(format!("BVAGENT SHARE paused {reason} limit={limit}"));
        }
    }

    /// A guest listing is applied: a paused share goes on.
    pub(in crate::agent_console) fn note_listing_applied(&mut self) {
        if std::mem::take(&mut self.listing_too_large) {
            print_pause_line("BVAGENT SHARE resumed".into());
        }
    }

    /// Pause lines printed on this test thread since the previous call.
    #[cfg(test)]
    pub(in crate::agent_console) fn take_pause_lines() -> Vec<String> {
        PAUSE_LINES.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
    }
}

fn print_pause_line(line: String) {
    println!("{line}");
    #[cfg(test)]
    PAUSE_LINES.with(|lines| lines.borrow_mut().push(line));
}

#[cfg(test)]
thread_local! {
    static PAUSE_LINES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
